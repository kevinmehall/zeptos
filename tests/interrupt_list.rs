#![allow(dead_code, unused_unsafe)]
use std::task::Waker;
use std::debug_assert;

#[derive(Copy, Clone)]
pub struct Runtime;

struct Interrupt;
impl Interrupt {
    pub const fn new() -> Self {
        Interrupt
    }
    pub fn subscribe(&self, _waker: &Waker) {}
    pub unsafe fn notify(&self) {}
}

mod executor {
    pub trait UntilOutput {
        type Output;
        fn into_output(self) -> Option<Self::Output>;
    }
}

mod interrupt_list {
    include!("../src/executor/interrupt_list.rs");

    #[test]
    fn test_interrupt_list() {
        use std::pin::pin;
        use std::task::Waker;

        let list = pin!(InterruptList::new());

        let node1 = pin!(Node::new(list.as_ref()));
        node1.as_ref().link(&Waker::noop());
        node1.as_ref().link(&Waker::noop());

        assert_eq!(list.head.get(), Some(NonNull::from_ref(node1.as_ref().get_ref())));
        assert_eq!(node1.prev.get(), None);
        assert_eq!(node1.next.get(), None);

        {
            let node2 = pin!(Node::new(list.as_ref()));
            node2.as_ref().link(&Waker::noop());

            assert_eq!(list.head.get(), Some(NonNull::from_ref(node2.as_ref().get_ref())));
            assert_eq!(node2.prev.get(), None);
            assert_eq!(node2.next.get(), Some(NonNull::from_ref(node1.as_ref().get_ref())));
            assert_eq!(node1.prev.get(), Some(NonNull::from_ref(node2.as_ref().get_ref())));
            assert_eq!(node1.next.get(), None);
        }

        assert_eq!(list.head.get(), Some(NonNull::from_ref(node1.as_ref().get_ref())));
        assert_eq!(node1.prev.get(), None);
        assert_eq!(node1.next.get(), None);

        unsafe { list.notify_all(); }

        assert_eq!(list.head.get(), None);
        assert_eq!(node1.prev.get(), None);
        assert_eq!(node1.next.get(), None);

        let node3 = pin!(Node::new(list.as_ref()));
        node3.as_ref().link(&Waker::noop());
        node1.as_ref().link(&Waker::noop());

        unsafe { list.notify_all(); }
    }
}
