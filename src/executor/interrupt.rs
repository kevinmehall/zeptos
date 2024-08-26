use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use crate::Runtime;

use super::RunQueueNode;

pub struct Interrupt {
    poll_fn: Cell<Option<unsafe fn()>>,
}

impl Interrupt {
    pub const fn new() -> Self {
        Self {
            poll_fn: Cell::new(None),
        }
    }

    fn subscribe(&self, waker: &Waker) {
        if waker.as_raw().vtable() != &super::VTABLE {
            panic!("interrupt passed a waker from another executor");
        }
        let node = unsafe { &*(waker.as_raw().data() as *mut RunQueueNode) };
        self.poll_fn.set(Some(node.func()))
    }

    pub unsafe fn notify(&self) {
        if let Some(poll) = self.poll_fn.take() {
            unsafe { poll() }
        }
    }

    pub fn until<'a, F: Fn() -> bool>(&'a self, condition: F) -> Until<'a, F> {
        Until {
            interrupt: self,
            condition,
        }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Until<'a, F> {
    interrupt: &'a Interrupt,
    condition: F,
}

impl<F: Fn() -> bool> Future for Until<'_, F> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if (self.condition)() {
            Poll::Ready(())
        } else {
            self.interrupt.subscribe(cx.waker());
            Poll::Pending
        }
    }
}

/// Send + Sync wrapper for a value that is not Send + Sync but can only be accessed from a task.
#[repr(transparent)]
pub struct TaskOnly<T>(T);

impl<T> TaskOnly<T> {
    /// Wrap a value.
    ///
    /// SAFETY: This is the equivalent of sending T to the
    /// task thread. You must either be on the task thread
    /// or T must be Send.
    pub const unsafe fn new(v: T) -> Self {
        TaskOnly(v)
    }

    /// Get the wrapped value.
    ///
    /// SAFETY: must only be called from inside a task,
    /// and not another core or an ISR at higher privilige.
    pub const unsafe fn get_unchecked(&self) -> &T {
        &self.0
    }

    /// Get the wrapped value.
    pub const fn get(&self, _runtime: Runtime) -> &T {
        unsafe { self.get_unchecked() }
    }
}

unsafe impl<T> Send for TaskOnly<T> {}
unsafe impl<T> Sync for TaskOnly<T> {}

#[macro_export]
macro_rules! isr {
    ($attr:ident, $name:ident) => {{
        use ::cortex_m_rt::$attr;

        static I: $crate::executor::TaskOnly<$crate::executor::Interrupt> =
            unsafe { $crate::executor::TaskOnly::new($crate::executor::Interrupt::new()) };

        #[$attr]
        fn $name() {
            unsafe { I.get().notify() }
        }

        unsafe { I.get() }
    }};
}

#[macro_export]
macro_rules! interrupt {
    ($name:ident) => {
        $crate::isr!(interrupt, $name)
    };
}

#[macro_export]
macro_rules! exception {
    ($name:ident) => {
        $crate::isr!(exception, $name)
    };
}
