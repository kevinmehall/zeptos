use core::{cell::Cell, future::Future, pin::Pin, ptr, task::{Context, Poll, RawWaker, RawWakerVTable, Waker}};

pub struct Interrupt {
    waker: Cell<Option<Waker>>
}

impl Interrupt {
    pub const fn new() -> Self {
        Self { waker: Cell::new(None) }
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

#[repr(transparent)]
pub struct RacyCell<T>(T);

impl<T> RacyCell<T> {
    pub const fn new(v: T) -> Self {
        RacyCell(v)
    }
    pub const unsafe fn get(&self) -> &T {
        &self.0
    }
}

unsafe impl<T> Send for RacyCell<T> {}
unsafe impl<T> Sync for RacyCell<T> {}

#[macro_export]
macro_rules! isr {
    ($attr:path, $name:ident) => {{
        static I: $crate::RacyCell<$crate::Interrupt> = $crate::RacyCell::new($crate::Interrupt::new());

        #[$attr]
        fn $name() {
            unsafe { I.get() }.notify()
        }

        unsafe { I.get() }
    }}
}

#[macro_export]
macro_rules! interrupt {
    ($name:ident) => { $crate::isr!(interrupt, $name) }
}

#[macro_export]
macro_rules! exception {
    ($name:ident) => { $crate::isr!(exception, $name) }
}

static NO_OP_VTABLE: RawWakerVTable = RawWakerVTable::new(
    |d| RawWaker::new(d, &NO_OP_VTABLE),
    |_| {},
    |_| {},
    |_| {},
);


