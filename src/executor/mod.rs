use cortex_m_rt::exception;
use cortex_m::peripheral::SCB;

mod interrupt;
use core::{cell::{Cell, UnsafeCell}, future::Future, mem::{ManuallyDrop, MaybeUninit}, pin::Pin, ptr::drop_in_place, task::{Context, Poll, RawWaker, RawWakerVTable, Waker}};

pub use interrupt::{Interrupt, TaskOnly};

mod runqueue;
pub use runqueue::{RunQueue, RunQueueNode};

static RUN_QUEUE: RunQueue = RunQueue::new();

/// Trait for a ZST that represents a task
pub trait Task: Sized + 'static {
    type Fut: Future + 'static;

    unsafe fn poll();
    fn node() -> &'static RunQueueNode;
    fn storage() -> &'static TaskStorage<Self>;
}

static VTABLE: RawWakerVTable = RawWakerVTable::new(
    waker_clone,
    waker_wake,
    waker_wake,
    drop
);

pub unsafe fn waker_clone(d: *const ()) -> RawWaker {
    RawWaker::new(d, &VTABLE)
}

pub unsafe fn waker_wake(p: *const ()) {
    let node = unsafe { &*(p as *const RunQueueNode) };
    RUN_QUEUE.enqueue(node);
    SCB::set_pendsv();
}

#[repr(C)]
pub struct TaskStorage<T: Task> {
    state: Cell<TaskState>,
    fut: UnsafeCell<MaybeUninit<T::Fut>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskState {
    Dead,
    Running,
    Polling,
}

unsafe impl<T: Task> Send for TaskStorage<T> {}
unsafe impl<T: Task> Sync for TaskStorage<T> {}

impl<T: Task> TaskStorage<T> {
    pub const fn new() -> Self {
        TaskStorage {
            state: Cell::new(TaskState::Dead),
            fut: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// SAFETY: must be called from the runtime thread and the task must be in state idle
    pub(crate) unsafe fn drop(&self) {
        unsafe {
            drop_in_place((*self.fut.get()).as_mut_ptr());
        }
        self.state.set(TaskState::Dead);
    }

    /// SAFETY: must be called from the runtime thread
    pub unsafe fn cancel(&self) { unsafe {
        match self.state.get() {
            TaskState::Dead => {}
            TaskState::Running => self.drop(),
            TaskState::Polling => panic!("task canceled itself"),
        }
    }}

    /// SAFETY: must be called from the runtime thread
    pub unsafe fn spawn(&'static self, fut: T::Fut) {
        unsafe {
            self.cancel();
            self.state.set(TaskState::Running);
            (*self.fut.get()).write(fut);
            T::poll()
        }
    }

    /// SAFETY: must be called from the runtime thread
    pub unsafe fn is_running(&self) -> bool {
        self.state.get() != TaskState::Dead
    }

    /// SAFETY: must be called from the runtime thread
    pub unsafe fn poll(&'static self) {
        if self.state.get() == TaskState::Running {
            self.state.set(TaskState::Polling);

            // Safety: If state was Running, we know the future is initialized, and we are not inside another call to poll.
            let mut fut = unsafe { Pin::new_unchecked((*self.fut.get()).assume_init_mut()) };

            // Our waker does not need to be dropped, so avoid emitting a drop call
            let waker = ManuallyDrop::new(
                unsafe { Waker::new(T::node() as *const _ as *mut _, &VTABLE) }
            );

            match fut.as_mut().poll(&mut Context::from_waker(&waker)) {
                Poll::Ready(_) => {
                    drop(fut);
                    unsafe { self.drop(); }
                }
                Poll::Pending => {
                    self.state.set(TaskState::Running);
                },
            }
        }
    }
}

#[exception]
fn PendSV() {
    unsafe {
        RUN_QUEUE.run_all()
    }
}


