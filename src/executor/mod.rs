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
    pub(crate) running: Cell<bool>,
    pub(crate) fut: UnsafeCell<MaybeUninit<T::Fut>>,
}

unsafe impl<T: Task> Send for TaskStorage<T> {}
unsafe impl<T: Task> Sync for TaskStorage<T> {}

impl<T: Task> TaskStorage<T> {
    pub const fn new() -> Self {
        TaskStorage {
            running: Cell::new(false),
            fut: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// SAFETY: must be called from the runtime thread and the task must be running
    pub(crate) unsafe fn drop(&self) {
        unsafe {
            drop_in_place((*self.fut.get()).as_mut_ptr());
        }
        self.running.set(false);
    }

    /// SAFETY: must be called from the runtime thread
    pub unsafe fn cancel(&self) {
        if self.running.get() {
            self.drop()
        }
    }

    /// SAFETY: must be called from the runtime thread
    pub unsafe fn spawn(&'static self, fut: T::Fut) {
        unsafe {
            self.cancel();
            self.running.set(true);
            (*self.fut.get()).write(fut);
            T::poll()
        }
    }

    /// SAFETY: must be called from the runtime thread
    pub unsafe fn is_running(&self) -> bool {
        self.running.get()
    }

    /// SAFETY: must be called from the runtime thread, and must not be called re-entrantly
    pub unsafe fn poll(&'static self) {
        if self.running.get() {
            // Safety: If state is Idle, we know the future is initialized, and we are not inside another call to poll.
            let mut fut = unsafe { Pin::new_unchecked((*self.fut.get()).assume_init_mut()) };

            let waker = ManuallyDrop::new(Waker::from_raw(RawWaker::new(T::node() as *const _ as *mut _, &VTABLE)));

            match fut.as_mut().poll(&mut Context::from_waker(&waker)) {
                Poll::Ready(_) => {
                    drop(fut);
                    self.drop();
                }
                Poll::Pending => {},
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


