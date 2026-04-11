use core::{
    cell::Cell,
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use super::RunQueueNode;

/// Event handling primitive for one task to wait for an interrupt.
///
/// This is normally placed in a `static`. An ISR can call `notify` to
/// wake the task that is waiting on the future returned by `until`.
pub struct Interrupt {
    poll_fn: Cell<unsafe fn()>,
}

fn no_op() {}

impl Interrupt {
    pub const fn new() -> Self {
        Self {
            poll_fn: Cell::new(no_op),
        }
    }

    pub fn subscribe(&self, waker: &Waker) {
        if waker.vtable() != &super::VTABLE {
            panic!("interrupt passed a waker from another executor");
        }
        let node = unsafe { &*(waker.data() as *mut RunQueueNode) };
        self.poll_fn.set(node.func())
    }

    pub unsafe fn notify(&self) {
        let poll = self.poll_fn.replace(no_op);
        unsafe { poll() }
    }

    pub fn until<'a, F: FnMut() -> R, R: UntilOutput>(&'a self, condition: F) -> Until<'a, F> {
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

impl<F: FnMut() -> R + Unpin, R: UntilOutput> Future for Until<'_, F> {
    type Output = R::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if let Some(r) = (this.condition)().into_output() {
            Poll::Ready(r)
        } else {
            this.interrupt.subscribe(cx.waker());
            Poll::Pending
        }
    }
}
