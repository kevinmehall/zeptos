use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use super::RunQueueNode;

/// Event handling primitive for waiting for an interrupt.
///
/// This is normally placed in a `static`. An ISR can call `notify` to
/// wake the task that is waiting on the future returned by `until`.
pub struct Interrupt {
    poll_fn: Cell<Option<unsafe fn()>>,
}

impl Interrupt {
    pub const fn new() -> Self {
        Self {
            poll_fn: Cell::new(None),
        }
    }

    pub fn subscribe(&self, waker: &Waker) {
        if waker.vtable() != &super::VTABLE {
            panic!("interrupt passed a waker from another executor");
        }
        let node = unsafe { &*(waker.data() as *mut RunQueueNode) };
        self.poll_fn.set(Some(node.func()))
    }

    pub unsafe fn notify(&self) {
        if let Some(poll) = self.poll_fn.take() {
            unsafe { poll() }
        }
    }

    pub fn until<'a, F: Fn() -> R, R: UntilOutput>(&'a self, condition: F) -> Until<'a, F> {
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

pub trait UntilOutput {
    type Output;

    fn into_output(self) -> Option<Self::Output>;
}

impl UntilOutput for bool {
    type Output = ();

    fn into_output(self) -> Option<Self::Output> {
        self.then_some(())
    }
}

impl<T> UntilOutput for Option<T> {
    type Output = T;

    fn into_output(self) -> Option<Self::Output> {
        self
    }
}

impl<F: Fn() -> R, R: UntilOutput> Future for Until<'_, F> {
    type Output = R::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(r) = (self.condition)().into_output() {
            Poll::Ready(r)
        } else {
            self.interrupt.subscribe(cx.waker());
            Poll::Pending
        }
    }
}
