#![no_std]
#![feature(impl_trait_in_assoc_type)]

use core::marker::PhantomData;

pub use zeptos_macros::main_cortex_m as main;
pub use zeptos_macros::task;

pub mod executor;

pub mod cortex_m;

#[cfg(any(feature="samd11", feature="samd21"))]
pub mod samd;

#[cfg(any(feature="samd11", feature="samd21"))]
pub use samd::{serial_number, SERIAL_NUMBER_LEN};

#[cfg(any(feature="rp2040"))]
pub mod rp;

#[cfg(any(feature="rp2040"))]
pub use rp::{serial_number, SERIAL_NUMBER_LEN};

#[cfg(any(feature="usb"))]
pub mod usb;

#[cfg(any(
    feature="samd-clock-48m-usb",
    feature="samd-clock-48m-internal",
    feature="samd-clock-48m-external-32k-osc",
    feature="samd-clock-48m-external-32k-xtal",
))]
pub const CLOCK_HZ: u32 = 48_000_000;

#[cfg(feature="rp2040")]
pub const CLOCK_HZ: u32 = 125_000_000;

#[doc(hidden)]
pub mod internal {
    pub use cortex_m_rt;

    use crate::{ cortex_m::SysTick, Hardware, Runtime };

    #[inline(always)]
    pub unsafe fn pre_init(rt: Runtime) -> Hardware { unsafe {
        cortex_m::interrupt::disable();

        #[cfg(any(feature = "samd11", feature = "samd21"))]
        crate::samd::init();

        #[cfg(any(feature = "rp2040"))]
        crate::rp::init();
        
        Hardware {
            syst: SysTick::init(rt),

            #[cfg(all(feature = "usb"))]
            usb: crate::usb::Usb::new(rt),
        }
    }}

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

    #[cfg(feature = "usb")]
    pub usb: usb::Usb,
}
