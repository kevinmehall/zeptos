use core::{ptr::{self, NonNull}, sync::atomic::{AtomicPtr, Ordering}};

const UNLINKED: *mut RunQueueNode = usize::MAX as *mut _;

pub struct RunQueue {
    head: AtomicPtr<RunQueueNode>
}

pub struct RunQueueNode {
    next: AtomicPtr<RunQueueNode>,
    func: unsafe fn(),
}

impl RunQueue {
    pub const fn new() -> RunQueue {
        Self { head: AtomicPtr::new(ptr::null_mut()) }
    }

    pub fn enqueue(&self, node: &'static RunQueueNode) {
        // TODO: use compare_exchange_weak on architectures with atomics
        if node.next.load(Ordering::Relaxed) == UNLINKED {
            let next = self.head.load(Ordering::Relaxed);
            node.next.store(next, Ordering::Relaxed);
            self.head.store(node as *const RunQueueNode as *mut _, Ordering::Relaxed);
        }
    }

    pub unsafe fn run_all(&self) {
        let head = self.head.load(Ordering::Relaxed);
        self.head.store(ptr::null_mut(), Ordering::Relaxed);

        let mut next = NonNull::new(head);
        while let Some(node) = next {
            let node = unsafe { node.as_ref() };

            // TODO: use swap on architectures with atomics
            next = NonNull::new(node.next.load(Ordering::Relaxed));
            node.next.store(UNLINKED, Ordering::Relaxed);

            unsafe {
                (node.func)();
            }
        }
    }
}

impl RunQueueNode {
    pub const fn new(func: unsafe fn()) -> RunQueueNode {
        Self { next: AtomicPtr::new(UNLINKED), func }
    }

    pub fn func(&self) -> unsafe fn() {
        self.func
    }
}

