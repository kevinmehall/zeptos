use core::task::{Context, Waker, RawWaker, RawWakerVTable};
use core::mem::{ ManuallyDrop, MaybeUninit };
use core::pin::Pin;
use core::ptr::drop_in_place;
use core::future::Future;
use core::cell::{ UnsafeCell, Cell };

#[derive(Clone, Copy)]
pub(crate) enum TaskState {
    Dead,
    Idle,
    Polling,
    PollingPending,
}

/// Trait for a ZST that represents a task
pub trait Task: Sized + 'static {
    type Fut: Future + 'static;
    
    fn storage() -> &'static TaskStorage<Self>;
    fn vtable() -> &'static RawWakerVTable;

    unsafe fn poll_fn() {
        Self::storage().poll()
    }
}


#[repr(C)]
pub struct TaskStorage<T: Task> {
    pub(crate) state: Cell<TaskState>,
    pub(crate) fut: UnsafeCell<MaybeUninit<T::Fut>>,
}

unsafe impl<T: Task> Send for TaskStorage<T> {}

unsafe impl<T: Task> Sync for TaskStorage<T> {}

impl<T: Task> TaskStorage<T> {
    pub unsafe fn waker_clone(d: *const ()) -> RawWaker {
        RawWaker::new(d, T::vtable())
    }

    pub unsafe fn waker_wake(_: *const ()) {
        T::poll_fn()
    }

    pub const fn new() -> Self {
        TaskStorage {
            state: Cell::new(TaskState::Dead),
            fut: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    pub(crate) unsafe fn drop(&self) {
        unsafe {
            drop_in_place((*self.fut.get()).as_mut_ptr());
        }
        self.state.set(TaskState::Dead);
    }

    pub unsafe fn cancel(&self) {
        match self.state.get() {
            TaskState::Dead => {},
            TaskState::Idle => unsafe { self.drop() },
            TaskState::Polling | TaskState::PollingPending => panic!("Can't cancel task while polling it.")
        }
    }

    pub unsafe fn spawn(&'static self, fut: T::Fut) {
        unsafe {
            self.cancel();
            self.state.set(TaskState::Idle);
            (*self.fut.get()).write(fut);
            T::poll_fn()
        }
    }

    pub unsafe fn is_running(&self) -> bool {
        match self.state.get() {
            TaskState::Dead => false,
            TaskState::Idle | TaskState::Polling | TaskState::PollingPending => true,
        }
    }

    pub(crate) unsafe fn poll(&'static self) {
        match self.state.get() {
            TaskState::Dead => return,
            TaskState::Polling | TaskState::PollingPending => {
                self.state.set(TaskState::PollingPending);
                return;
            }
            TaskState::Idle => {
                // Safety: If state is Idle, we know the future is initialized, and we are not inside another call to poll.
                let mut fut = unsafe { Pin::new_unchecked((*self.fut.get()).assume_init_mut()) };

                let waker = ManuallyDrop::new(Waker::from_raw(RawWaker::new(self as *const Self as *const (), T::vtable()))); // TODO: LocalWaker

                loop {
                    self.state.set(TaskState::Polling);

                    if fut.as_mut().poll(&mut Context::from_waker(&waker)).is_ready() {
                        self.state.set(TaskState::Dead);
                        drop(fut);
                        self.drop();
                        return;
                    }

                    match self.state.get() {
                        TaskState::Dead | TaskState::Idle => unreachable!(),
                        TaskState::Polling => break,
                        TaskState::PollingPending => continue,
                    }
                }
                self.state.set(TaskState::Idle);
            }
        }
    }
}
