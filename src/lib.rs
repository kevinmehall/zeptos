#![no_std]

use core::cell::{Cell, UnsafeCell};
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::pin::Pin;
use core::ptr::drop_in_place;
use core::task::{Context, RawWaker, RawWakerVTable, Waker};
use core::future::Future;

pub use zeptos_macros::main_cortex_m as main;
pub use zeptos_macros::task;

mod interrupt;
pub use interrupt::{RacyCell, Interrupt};

#[doc(hidden)]
pub mod internal {
    pub use cortex_m_rt;

    #[inline(always)]
    pub unsafe fn pre_init() {
        cortex_m::interrupt::disable();
    }

    #[inline(always)]
    pub unsafe fn post_init() -> ! {
        unsafe {
            cortex_m::interrupt::enable();
        }
        loop {
            cortex_m::asm::wfi();
        }
    }
}

#[derive(Copy, Clone)]
pub struct Spawner {
    _not_send: PhantomData<*mut ()>,
}

impl Spawner {
    pub unsafe fn steal() -> Spawner {
        Spawner { _not_send: PhantomData }
    }
}

#[derive(Clone, Copy)]
enum TaskState {
    Dead,
    Idle,
    Polling,
    PollingPending,
}

pub struct Task<F> {
    state: Cell<TaskState>,
    fut: UnsafeCell<MaybeUninit<F>>,
    vtable: RawWakerVTable,
}

unsafe impl<F> Send for Task<F> {}
unsafe impl<F> Sync for Task<F> {}

impl<F: Future + 'static> Task<F> {
    pub const fn new() -> Self {
        Task {
            state: Cell::new(TaskState::Dead),
            fut: UnsafeCell::new(MaybeUninit::uninit()),
            vtable: RawWakerVTable::new(
                |d| {
                    let task: &'static Self = unsafe { &*(d as *const Self) };
                    RawWaker::new(d, &task.vtable)
                },
                |d| {
                    unsafe { (*(d as *const Self)).poll() }
                },
                |d| {
                    unsafe { (*(d as *const Self)).poll() }
                }
            , drop)
        }
    }

    unsafe fn drop(&self) {
        unsafe {
            drop_in_place((*self.fut.get()).as_mut_ptr());
        }
        self.state.set(TaskState::Dead);
    }

    pub unsafe fn cancel(&self) {
        match self.state.get() {
            TaskState::Dead => {},
            TaskState::Idle => self.drop(),
            TaskState::Polling | TaskState::PollingPending => panic!("Can't cancel task while polling it.")
        }
    }

    pub unsafe fn spawn(&'static self, fut: F) {
        unsafe {
            self.cancel();
            self.state.set(TaskState::Idle);
            (*self.fut.get()).write(fut);
            self.poll();
        }
    }

    pub unsafe fn is_running(&self) -> bool {
        match self.state.get() {
            TaskState::Dead => false,
            TaskState::Idle | TaskState::Polling | TaskState::PollingPending => true,
        }
    }

    unsafe fn poll(&'static self) {
        match self.state.get() {
            TaskState::Dead => return,
            TaskState::Polling | TaskState::PollingPending => {
                self.state.set(TaskState::PollingPending);
                return;
            }
            TaskState::Idle => {
                // Safety: If state is Idle, we know the future is initialized, and we are not inside another call to poll.
                let mut fut = unsafe { Pin::new_unchecked((*self.fut.get()).assume_init_mut()) };

                let waker = ManuallyDrop::new(Waker::from_raw(RawWaker::new(self as *const Self as *const (), &self.vtable))); // TODO: LocalWaker

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
