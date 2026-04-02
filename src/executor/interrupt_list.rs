use core::{cell::Cell, marker::{PhantomData, PhantomPinned}, pin::Pin, ptr::NonNull, task::{Context, Poll, Waker}};

use crate::Interrupt;
use crate::executor::UntilOutput;
use crate::debug_assert;

/// Event handling primitive for multiple tasks to wait for an interrupt.
///
/// This is normally placed in a `static`. An ISR can call `notify` to
/// wake all tasks that are waiting on the future returned by `until`.
pub struct InterruptList {
    // If set, all nodes reachable from this node are valid and linked in a circular list.
    head: Cell<Option<NonNull<Node>>>,
}

struct Node {
    _pinned: PhantomPinned,

    // Invariant: these pointers are valid if not None
    prev: Cell<Option<NonNull<Node>>>,
    next: Cell<Option<NonNull<Node>>>,
    list: NonNull<InterruptList>,
    linked: Cell<bool>,

    waker: Interrupt,
}

impl InterruptList {
    pub const fn new() -> Self {
        InterruptList {
            head: Cell::new(None),
        }
    }

    fn head(&self) -> Option<&Node> {
        // SAFETY: by the field invariant, all reachable nodes are valid
        self.head.get().map(|ptr| unsafe { ptr.as_ref() })
    }

    /// SAFETY: Must be called from an ISR at the runtime priority, but not within a task.
    pub unsafe fn notify_all(&self) {
        // SAFETY: valid by invariant of `head`.
        let mut head = self.head.take().map(|head| unsafe { head.as_ref() });
        while let Some(node) = head {
            // Unlink the node from the list before notifying, as the task may re-link itself
            debug_assert!(node.prev.get().is_none());
            debug_assert!(node.linked.get());
            node.next.set(None);
            node.linked.set(false);

            head = node.next();

            // The task might drop nodes later in the list, so detach the next node so it doesn't touch this one via prev
            if let Some(n) = head {
                n.prev.set(None);
            }

            // SAFETY: same as this function's safety requirement.
            unsafe { node.waker.notify(); }
        }
    }

    pub fn until<'a, F: Fn() -> R, R: UntilOutput>(self: Pin<&'a Self>, condition: F) -> Until<'a, F> {
        Until {
            node: Node::new(self),
            p: PhantomData,
            condition,
        }
    }
}

impl Node {
    pub const fn new(list: Pin<&InterruptList>) -> Self {
        Node {
            _pinned: PhantomPinned,
            prev: Cell::new(None),
            next: Cell::new(None),
            list: NonNull::from_ref(list.get_ref()),
            linked: Cell::new(false),
            waker: Interrupt::new(),
        }
    }

    fn list(&self) -> &InterruptList {
        unsafe { self.list.as_ref() }
    }

    fn prev(&self) -> Option<&Node> {
        // SAFETY: by the field invariant, all reachable nodes are valid
        self.prev.get().map(|ptr| unsafe { ptr.as_ref() })
    }

    fn next(&self) -> Option<&Node> {
        // SAFETY: by the field invariant, all reachable nodes are valid
        self.next.get().map(|ptr| unsafe { ptr.as_ref() })
    }

    fn link(self: Pin<&Self>, waker: &Waker) {
        self.waker.subscribe(waker);
        if !self.linked.get() {
            debug_assert!(self.prev.get().is_none());
            debug_assert!(self.next.get().is_none());
            self.next.set(self.list().head.get());
            if let Some(head) = self.list().head() {
                head.prev.set(Some(NonNull::from_ref(self.get_ref())));
            }
            self.list().head.set(Some(NonNull::from_ref(self.get_ref())));
            self.linked.set(true);
        }
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Some(prev) = self.prev() {
            prev.next.set(self.next.get());
        } else if self.list().head.get() == Some(NonNull::from(&*self)) {
            self.list().head.set(self.next.get());
        } else {
            // If prev is `None` but the head of the list doesn't point back at us, then we're at the
            // front of the detached portion of the list in `notify_all`.
        }

        if let Some(next) = self.next() {
            next.prev.set(self.prev.get());
        }
    }
}

#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Until<'a, F> {
    /// Structurally pinned
    node: Node,
    p: PhantomData<&'a InterruptList>,
    condition: F,
}

impl<F: Fn() -> R, R: UntilOutput> Future for Until<'_, F> {
    type Output = R::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(r) = (self.condition)().into_output() {
            Poll::Ready(r)
        } else {
            // SAFETY: structural pin projection
            let node = unsafe { Pin::new_unchecked(&self.node) };
            node.link(cx.waker());
            Poll::Pending
        }
    }
}
