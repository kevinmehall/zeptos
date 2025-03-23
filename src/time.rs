use core::{cell::Cell, marker::PhantomPinned, pin::Pin, ptr::NonNull, task::{Context, Poll}};

use defmt::Format;

use crate::{executor::{Interrupt, TaskOnly}, Runtime};

use super::cortex_m::systick::{now as hw_now, schedule as hw_schedule, init as hw_init};

pub(crate) fn init() {
    hw_init();
}

/// Timestamp in microseconds since boot.
/// 
/// This wraps after 2^32 microseconds, or about 71 minutes.
#[derive(Copy, Clone, Format, Eq, PartialEq)]
pub struct Instant(pub u32);

impl Instant {
    pub const fn is_before(&self, other: Self) -> bool {
        !(other.0.wrapping_sub(self.0) > 0x8000_0000)
    }

    pub const fn add_us(&self, us: u32) -> Self {
        Instant(self.0.wrapping_add(us))
    }
}

impl Runtime {
    /// Get the current time.
    #[inline]
    pub fn now(&self) -> Instant {
        hw_now()
    }

    /// Wait until the given time.
    #[inline]
    pub fn delay_until(&self, target: Instant) -> Wait {
        Wait::new(*self, target)
    }

    /// Delay for at least the given number of microseconds.
    #[inline]
    pub fn delay_us(&self, us: u32) -> Wait {
        Wait::new(*self, self.now().add_us(us))
    }
}

static HEAD: TaskOnly<Cell<Option<NonNull<Wait>>>> = unsafe { TaskOnly::new(Cell::new(None)) };

#[must_use]
pub struct Wait {
    rt: Runtime,
    target: Instant,
    prev: Cell<Option<NonNull<Wait>>>,
    next: Cell<Option<NonNull<Wait>>>,
    linked: Cell<bool>,
    waker: Interrupt,
    _pinned: PhantomPinned,
}

impl Wait {
    pub const fn new(rt: Runtime, target: Instant) -> Self {
        Wait {
            rt,
            _pinned: PhantomPinned,
            target,
            prev: Cell::new(None),
            next: Cell::new(None),
            linked: Cell::new(false),
            waker: Interrupt::new(),
        }
    }

    fn prev(&self) -> Option<&Wait> {
        self.prev.get()
            .map(|ptr| unsafe { ptr.as_ref() })
    }

    fn next(&self) -> Option<&Wait> {
        self.next.get()
            .map(|ptr| unsafe { ptr.as_ref() })
    }
    
    fn link(self: Pin<&mut Self>) {
        if !self.linked.get() {    
            defmt::trace!("linking timer at {=u32}", self.target.0);
            let mut node_ref = HEAD.get(self.rt);
            let mut prev = None;
    
            while let Some(node_ptr) = node_ref.get() {
                let node = unsafe { node_ptr.as_ref() };
    
                if self.target.is_before(node.target) {
                    node.prev.set(Some(NonNull::from(self.as_ref().get_ref())));
                    break;
                }
    
                prev = Some(node_ptr);
                node_ref = &node.next;
            }
    
            self.prev.set(prev);
            self.next.set(node_ref.get());
            node_ref.set(Some(NonNull::from(self.as_ref().get_ref())));
            self.linked.set(true);

            schedule(self.rt);
        }
    }
}

impl Future for Wait {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let now = self.rt.now();

        if self.target.is_before(now) {
            return Poll::Ready(());
        }

        self.waker.subscribe(cx.waker());
        self.link();

        Poll::Pending
    }
}

impl Drop for Wait {
    fn drop(&mut self) {
        if self.linked.get() {
            if let Some(prev) = self.prev() {
                prev.next.set(self.next.get());
            } else {
                HEAD.get(self.rt).set(self.next.get());
                schedule(self.rt);
            }

            if let Some(next) = self.next() {
                next.prev.set(self.prev.get());
            }
        }
    }
}

/// Timer callback to wake and execute expired timers.
/// 
/// Safety: must not be called from within a task.
pub(crate) unsafe fn tick(rt: Runtime, now: Instant) {
    let head = HEAD.get(rt);

    while let Some(node_ptr) = head.get() {
        let node = unsafe { node_ptr.as_ref() };

        if now.is_before(node.target) {
            break;
        }

        head.set(node.next.get());
        if let Some(next) = node.next() {
            next.prev.set(None)
        }
        node.linked.set(false);

        defmt::trace!("notifying timer at {=u32}", node.target.0);
        
        unsafe {
            node.waker.notify();
        }
    }

    schedule(rt);
}

fn schedule(rt: Runtime) {
    let first = HEAD.get(rt).get().map(|head| {
        unsafe { head.as_ref() }.target
    });

    hw_schedule(first);
}
