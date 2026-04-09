//! A tiny runtime for async Rust on microcontrollers.
//!
//! The functionality of this crate is enabled by Cargo features:
//!
//! * `samd11` or `samd21`: Support for the SAM D11 or D21 microcontrollers.
//!     * `samd-clock-48m-usb`, `samd-clock-48m-internal`, `samd-clock-48m-external-32k-osc`, or `samd-clock-48m-external-32k-xtal`: Configure the clock source
//! * `rp2040`: Support for the Raspberry Pi RP2040 microcontroller.
//!    * `rp2040-boot2-w25q080`
//!    * `rom-func-cache`
//!
//! * `usb`: Enables USB support.
//! * `time`: Enables systick timer.
#![no_std]
#![feature(impl_trait_in_assoc_type, sync_unsafe_cell)]

use core::marker::PhantomData;

// Modules use via this re-export so it can be turned off when building for host in test.
#[allow(unused_imports)]
use defmt::{panic, assert, debug_assert};

pub use zeptos_macros::main_cortex_m as main;
pub use zeptos_macros::task;

mod executor;
pub use executor::{Interrupt, InterruptList, TaskOnly, TaskRef};

mod cortex_m;

cfg_select! {
    any(feature="samd11", feature="samd21") => {
        pub mod samd;
        pub use samd::{serial_number::{serial_number, SERIAL_NUMBER_LEN}};
    }
    any(feature="rp2040", feature="rp2350") => {
        pub mod rp;
        pub use rp::{serial_number::{serial_number, SERIAL_NUMBER_LEN}};
    }
    _ => {}
}

#[cfg(feature="time")]
pub mod time;

#[cfg(feature="time")]
use cortex_m::systick as timer_hw;

#[cfg(feature="usb")]
pub mod usb;

cfg_select!{
    any(
        feature="samd-clock-48m-usb",
        feature="samd-clock-48m-internal",
        feature="samd-clock-48m-external-32k-osc",
        feature="samd-clock-48m-external-32k-xtal",
    ) =>  {
        pub const CLOCK_HZ: u32 = 48_000_000;
    }
    feature="rp2040" => {
        pub const CLOCK_HZ: u32 = 125_000_000;
    }
    feature="rp2350" => {
        pub const CLOCK_HZ: u32 = 150_000_000;
    }
    _ => {}
}

/// Interface with the macro-generated code
#[doc(hidden)]
pub mod internal {
    pub use cortex_m_rt;

    #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "i2c0"))]
use crate::rp;
    pub use crate::{ Hardware, Runtime, executor::{ RunQueue, RunQueueNode, Task, TaskStorage } };

    #[inline(always)]
    pub unsafe fn pre_init(rt: Runtime) -> Hardware {
        let _ = rt;

        cortex_m::interrupt::disable();

        cfg_select! {
            any(feature = "samd11", feature = "samd21") => {
                crate::samd::init();
            }
            any(feature = "rp2040", feature = "rp2350") => {
                crate::rp::init();
            }
            _ => {}
        }

        #[cfg(feature = "time")]
        crate::time::init();

        Hardware {
            #[cfg(feature = "usb")]
            usb: unsafe { crate::usb::Usb::new(rt) },

            #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "i2c0"))]
            i2c0: unsafe { <rp::i2c::I2c0 as rp::i2c::StaticInstance>::steal() },

            #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "i2c1"))]
            i2c1: unsafe { <rp::i2c::I2c1 as rp::i2c::StaticInstance>::steal() },

            #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "spi0"))]
            spi0: unsafe { <rp::spi::Spi0 as rp::spi::StaticInstance>::steal() },

            #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "spi1"))]
            spi1: unsafe { <rp::spi::Spi1 as rp::spi::StaticInstance>::steal() },
        }
    }

    #[inline(always)]
    pub unsafe fn post_init() -> ! {
        unsafe {
            use cortex_m::peripheral::SCB;
            (*SCB::PTR).scr.write(0x1 << 1); // Enable SLEEPONEXIT
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
    /// Create a new `Runtime` token by assuming that we are running on the task thread.
    ///
    /// ## Safety
    /// Can only be called from inside a task, and not
    /// on another core or at a higher interrupt priority.
    pub unsafe fn steal() -> Runtime {
        Runtime {
            _not_send: PhantomData,
        }
    }
}

/// Exclusive access to peripherals passed to the main task.
///
/// The fields in this struct depend on the cargo features enabled.
pub struct Hardware {
    #[cfg(feature = "usb")]
    pub usb: usb::Usb,

    #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "i2c0"))]
    pub i2c0: rp::i2c::I2c0,

    #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "i2c1"))]
    pub i2c1: rp::i2c::I2c1,

    #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "spi0"))]
    pub spi0: rp::spi::Spi0,

    #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "spi1"))]
    pub spi1: rp::spi::Spi1,
}
