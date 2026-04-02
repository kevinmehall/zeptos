#![allow(dead_code, unused_unsafe)]
use std::task::Waker;

#[derive(Copy, Clone)]
pub struct Runtime;

pub struct TaskOnly<T>(T);
impl<T> TaskOnly<T> {
    pub const unsafe fn new_unsend(value: T) -> Self {
        TaskOnly(value)
    }
    pub fn get(&self, _rt: Runtime) -> &T {
        &self.0
    }
}

unsafe impl<T> Send for TaskOnly<T> {}
unsafe impl<T> Sync for TaskOnly<T> {}

struct Interrupt;
impl Interrupt {
    pub const fn new() -> Self {
        Interrupt
    }
    pub fn subscribe(&self, _waker: &Waker) {}
    pub unsafe fn notify(&self) {}
}

mod timer_hw {
    use crate::time::Instant;
    use core::sync::atomic::{AtomicU32, Ordering};
    pub static NOW: AtomicU32 = AtomicU32::new(0);
    pub static NEXT: AtomicU32 = AtomicU32::new(0);

    pub fn init() {}

    pub fn now() -> Instant {
        Instant(NOW.load(Ordering::Relaxed))
    }

    pub fn schedule(time: Option<Instant>) {
        NEXT.store(time.map_or(0, |t| t.0), Ordering::Relaxed);
    }
}

mod time {
    include!("../src/time.rs");

    #[test]
    pub fn test_instant() {
        let t0 = Instant(0xFFFF_FFFE);
        let t1 = Instant(1000);
        let t2 = Instant(2000);

        assert!(t2.is_after(t1));
        assert!(!t1.is_after(t2));
        assert!(!t2.is_after(t2));

        assert!(t1.is_after(t0));
        assert!(!t0.is_after(t1));
    }

    #[test]
    pub fn test_queue() {
        use core::pin::pin;
        use core::sync::atomic::Ordering;
        use crate::timer_hw::NEXT;

        let rt = Runtime;

        let mut w1 = pin!(Wait::new(rt, Instant(1000)));
        w1.as_mut().link();
        assert_eq!(NEXT.load(Ordering::Relaxed), 1000);
        assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w1.as_ref().get_ref())));
        assert_eq!(w1.prev.get(), None);
        assert_eq!(w1.next.get(), None);

        let mut w2 = pin!(Wait::new(rt, Instant(2000)));
        w2.as_mut().link();
        assert_eq!(NEXT.load(Ordering::Relaxed), 1000);
        assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w1.as_ref().get_ref())));
        assert_eq!(w1.prev.get(), None);
        assert_eq!(w1.next.get(), Some(NonNull::from(w2.as_ref().get_ref())));
        assert_eq!(w2.prev.get(), Some(NonNull::from(w1.as_ref().get_ref())));
        assert_eq!(w2.next.get(), None);

        {
            let mut w3 = pin!(Wait::new(rt, Instant(1500)));
            w3.as_mut().link();
            assert_eq!(NEXT.load(Ordering::Relaxed), 1000);
            assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w1.as_ref().get_ref())));
            assert_eq!(w1.prev.get(), None);
            assert_eq!(w1.next.get(), Some(NonNull::from(w3.as_ref().get_ref())));
            assert_eq!(w3.prev.get(), Some(NonNull::from(w1.as_ref().get_ref())));
            assert_eq!(w3.next.get(), Some(NonNull::from(w2.as_ref().get_ref())));
            assert_eq!(w2.prev.get(), Some(NonNull::from(w3.as_ref().get_ref())));
            assert_eq!(w2.next.get(), None);

            {
                let mut w4 = pin!(Wait::new(rt, Instant(500)));
                w4.as_mut().link();
                assert_eq!(NEXT.load(Ordering::Relaxed), 500);
                assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w4.as_ref().get_ref())));
                assert_eq!(w4.prev.get(), None);
                assert_eq!(w4.next.get(), Some(NonNull::from(w1.as_ref().get_ref())));
                assert_eq!(w1.prev.get(), Some(NonNull::from(w4.as_ref().get_ref())));
                assert_eq!(w1.next.get(), Some(NonNull::from(w3.as_ref().get_ref())));

                let mut w5 = pin!(Wait::new(rt, Instant(1500)));
                w5.as_mut().link();
            }

            assert_eq!(NEXT.load(Ordering::Relaxed), 1000);
            assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w1.as_ref().get_ref())));
            assert_eq!(w1.prev.get(), None);
            assert_eq!(w1.next.get(), Some(NonNull::from(w3.as_ref().get_ref())));
        }

        assert_eq!(NEXT.load(Ordering::Relaxed), 1000);
        assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w1.as_ref().get_ref())));
        assert_eq!(w1.prev.get(), None);
        assert_eq!(w1.next.get(), Some(NonNull::from(w2.as_ref().get_ref())));
        assert_eq!(w2.prev.get(), Some(NonNull::from(w1.as_ref().get_ref())));
        assert_eq!(w2.next.get(), None);

        unsafe { tick(rt, Instant(10)) };
        assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w1.as_ref().get_ref())));

        unsafe { tick(rt, Instant(1000)) };
        assert_eq!(HEAD.get(rt).get(), Some(NonNull::from(w2.as_ref().get_ref())));
        assert_eq!(w1.linked.get(), false);
        assert_eq!(w2.prev.get(), None);
        assert_eq!(w2.next.get(), None);

        unsafe { tick(rt, Instant(2000)) };
        assert_eq!(HEAD.get(rt).get(), None);
        assert_eq!(w2.linked.get(), false);

        w1.as_mut().link();
        w2.as_mut().link();
        unsafe { tick(rt, Instant(2000)) };
        assert_eq!(HEAD.get(rt).get(), None);
    }
}
