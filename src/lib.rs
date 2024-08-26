#![no_std]
#![feature(waker_getters)]

use core::marker::PhantomData;

pub use zeptos_macros::main_cortex_m as main;
pub use zeptos_macros::task;

pub mod executor;

pub mod cortex_m;

#[cfg(any(feature="samd11", feature="samd21"))]
pub mod samd;

#[doc(hidden)]
pub mod internal {
    pub use cortex_m_rt;

    use crate::{ cortex_m::SysTick, Hardware, Runtime };

    #[inline(always)]
    pub unsafe fn pre_init(rt: Runtime) -> Hardware {
        cortex_m::interrupt::disable();

        Hardware {
            syst: SysTick::init(rt)
        }
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

pub struct Hardware {
    pub syst: cortex_m::SysTick,
}
