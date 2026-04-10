//! A tiny runtime for async Rust on microcontrollers.
//!
//! Zeptos turns the ARM Cortex-M NVIC into an executor for async Rust. It runs
//! entirely in handler mode: awaiting an interrupt means your task continues
//! execution from that interrupt handler. Execution bounces between interrupt
//! handlers, going to sleep rather than ever returning to thread mode. Because
//! ISRs at the same priority level run to completion without preemption, it's
//! single-threaded, with no need for any synchronization overhead anywhere.
//!
//! It requires Rust Nightly.
//!
//! The device-specific functionality of this crate is enabled by Cargo features:
//!
//! * `samd11` or `samd21`: Support for the Atmel / Microchip SAM D11 or D21 microcontrollers.
//!     * `samd-clock-48m-usb`, `samd-clock-48m-internal`, `samd-clock-48m-external-32k-osc`, or `samd-clock-48m-external-32k-xtal`: Configure the clock source
//!     * `sercom0`, `sercom1`, `sercom2`, `sercom3`, `sercom4`, or `sercom5`: Enable clocks and interrupts for the corresponding SERCOM peripheral, and add it to the `Hardware` struct passed to the main task.
//!
//! * `rp2040` or `rp2350`: Support for the Raspberry Pi RP2040 or RP2350 microcontroller.
//!    * `rp2040-boot2-w25q080` (RP2040 only): Use the W25Q080 bootloader for XIP on Raspberry Pi Pico.
//!    * `rom-func-cache` (RP2040 only): Enable ROM function cache.
//!    * `i2c0`, `i2c1`, `spi0`, or `spi1`: Enable clocks and interrupts for the corresponding peripheral, and add it to the `Hardware` struct passed to the main task.
//!    * `gpio-interrupts`: Enables GPIO interrupts.
//!
//! * `usb`: Enables USB support.
//! * `time`: Enables systick timer.
#![no_std]
#![feature(impl_trait_in_assoc_type, sync_unsafe_cell, doc_cfg)]

use core::marker::PhantomData;

// Modules use via this re-export so it can be turned off when building for host in test.
#[allow(unused_imports)]
use defmt::{panic, assert, debug_assert};

pub use zeptos_macros::main_cortex_m as main;
pub use zeptos_macros::task;

mod executor;
pub use executor::{Interrupt, InterruptList, TaskOnly, TaskRef};

mod cortex_m;

#[cfg(any(feature="samd11", feature="samd21"))]
pub mod samd;

#[cfg(any(feature="rp2040", feature="rp2350"))]
pub mod rp;

cfg_select! {
    any(feature="samd11", feature="samd21") => {
        pub use samd::{serial_number::{serial_number, SERIAL_NUMBER_LEN}};
    }
    any(feature="rp2040", feature="rp2350") => {
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
            i2c0: unsafe { <crate::rp::i2c::I2c0 as crate::rp::i2c::StaticInstance>::steal() },

            #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "i2c1"))]
            i2c1: unsafe { <crate::rp::i2c::I2c1 as crate::rp::i2c::StaticInstance>::steal() },

            #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "spi0"))]
            spi0: unsafe { <crate::rp::spi::Spi0 as crate::rp::spi::StaticInstance>::steal() },

            #[cfg(all(any(feature = "rp2040", feature = "rp2350"), feature = "spi1"))]
            spi1: unsafe { <crate::rp::spi::Spi1 as crate::rp::spi::StaticInstance>::steal() },

            #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom0"))]
            sercom0: unsafe { <crate::samd::sercom::Sercom0 as crate::samd::sercom::StaticSercom>::steal() },

            #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom1"))]
            sercom1: unsafe { <crate::samd::sercom::Sercom1 as crate::samd::sercom::StaticSercom>::steal() },

            #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom2"))]
            sercom2: unsafe { <crate::samd::sercom::Sercom2 as crate::samd::sercom::StaticSercom>::steal() },

            #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom3"))]
            sercom3: unsafe { <crate::samd::sercom::Sercom3 as crate::samd::sercom::StaticSercom>::steal() },

            #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom4"))]
            sercom4: unsafe { <crate::samd::sercom::Sercom4 as crate::samd::sercom::StaticSercom>::steal() },

            #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom5"))]
            sercom5: unsafe { <crate::samd::sercom::Sercom5 as crate::samd::sercom::StaticSercom>::steal() },
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

    #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom0"))]
    pub sercom0: samd::sercom::Sercom0,

    #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom1"))]
    pub sercom1: samd::sercom::Sercom1,

    #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom2"))]
    pub sercom2: samd::sercom::Sercom2,

    #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom3"))]
    pub sercom3: samd::sercom::Sercom3,

    #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom4"))]
    pub sercom4: samd::sercom::Sercom4,

    #[cfg(all(any(feature = "samd11", feature = "samd21"), feature = "sercom5"))]
    pub sercom5: samd::sercom::Sercom5,
}
