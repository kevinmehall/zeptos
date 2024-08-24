use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

pub struct Interrupt {
    waker: Cell<Option<Waker>>,
}

impl Interrupt {
    pub const fn new() -> Self {
        Self {
            waker: Cell::new(None),
        }
    }

    pub fn subscribe(&self, waker: Waker) {
        self.waker.set(Some(waker))
    }

    pub fn notify(&self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
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
            self.interrupt.subscribe(cx.waker().clone());
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
    pub const unsafe fn get(&self) -> &T {
        &self.0
    }
}

unsafe impl<T> Send for TaskOnly<T> {}
unsafe impl<T> Sync for TaskOnly<T> {}

#[macro_export]
macro_rules! isr {
    ($attr:ident, $name:ident) => {{
        use ::cortex_m_rt::$attr;

        static I: $crate::TaskOnly<$crate::Interrupt> =
            unsafe { $crate::TaskOnly::new($crate::Interrupt::new()) };

        #[$attr]
        fn $name() {
            unsafe { I.get() }.notify()
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
