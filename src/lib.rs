#![no_std]

use core::marker::PhantomData;

pub use zeptos_macros::main_cortex_m as main;
pub use zeptos_macros::task;

mod interrupt;
pub use interrupt::{Interrupt, TaskOnly};

mod task;
pub use task::{Task, TaskStorage};

pub mod cortex_m;

#[doc(hidden)]
pub mod internal {
    pub use cortex_m_rt;

    #[inline(always)]
    pub unsafe fn pre_init() {
        cortex_m::interrupt::disable();
    }

    #[inline(always)]
    pub unsafe fn post_init() -> ! {
        unsafe {
            cortex_m::interrupt::enable();
        }
        loop {
            cortex_m::asm::wfi();
        }
    }
}

/// A token whose possession proves that you are on the task thread
#[derive(Copy, Clone)]
pub struct Runtime {
    _not_send: PhantomData<*mut ()>,
}

impl Runtime {
    /// SAFETY: Can only be called from inside a task, and not
    /// on another core or at a higher interrupt priority.
    pub unsafe fn steal() -> Runtime {
        Runtime {
            _not_send: PhantomData,
        }
    }
}
