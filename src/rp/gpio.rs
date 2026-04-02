#[cfg(feature = "gpio-interrupts")]
use rp_pac::interrupt;
use rp_pac::{SIO, common::{RW, Reg}, io::Io};

#[cfg(feature = "gpio-interrupts")]
use crate::InterruptList;
#[allow(unused_imports)]
use crate::{Interrupt, Runtime, TaskOnly, rp::RpReg};

pub trait PinFunc {
    const DYN: Function;
}

macro_rules! alternate {
    ($($name:ident = $val:literal),+) => {
        #[repr(u8)]
        pub enum Function { $($name = $val),+ }
        pub mod alternate {
            use super::{Function, PinFunc};
            $(
                pub enum $name {}
                impl PinFunc for $name {
                    const DYN: Function = Function::$name;
                }
            )+
        }
    };
}
alternate!(F1 = 1, F2 = 2, F3 = 3, F4 = 4, F5 = 5, F6 = 6, F7 = 7, F8 = 8, F9 = 9);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IoPin {
    pub pin: u8,
}

impl IoPin {
    #[inline]
    pub fn bank0(pin: u8) -> Self {
        assert!(pin < 30);
        Self { pin }
    }

    fn bank(&self) -> usize {
        0
    }

    fn pin_in_bank(&self) -> usize {
        self.pin as usize
    }

    fn mask(&self) -> u32 {
        1 << self.pin_in_bank()
    }

    #[inline]
    fn pad(&self) -> Reg<rp_pac::pads::regs::GpioCtrl, RW> {
        crate::rp::pac::PADS_BANK0.gpio(self.pin_in_bank())
    }

    #[inline]
    fn io(&self) -> Io {
        crate::rp::pac::IO_BANK0
    }

    #[inline]
    pub fn set_function(&self, func: Function) {
        self.io().gpio(self.pin_in_bank()).ctrl().write(|w| {
            w.set_funcsel(func as u8);
        });

        self.pad().write(|w| {
            #[cfg(feature = "rp2350")]
            w.set_iso(false);
            w.set_ie(true);
        });
    }

    #[inline]
    pub fn disable(&self) {
        self.io().gpio(self.pin_in_bank()).ctrl().write(|w| {
            w.set_funcsel(31);
        });
    }

    #[inline]
    pub fn configure_pad(&self, pull_up: bool, pull_down: bool) {
        self.pad().write(|w| {
            #[cfg(feature = "rp2350")]
            w.set_iso(false);
            w.set_ie(true);
            w.set_pue(pull_up);
            w.set_pde(pull_down);
        });
    }

    #[inline]
    pub fn read(&self) -> bool {
        SIO.gpio_in(self.bank()).read() & self.mask() != 0
    }

    #[inline]
    pub fn read_out(&self) -> bool {
        SIO.gpio_out(self.bank()).value().read() & self.mask() != 0
    }

    #[inline]
    pub fn read_oe(&self) -> bool {
        SIO.gpio_oe(self.bank()).value().read() & self.mask() != 0
    }

    #[inline]
    pub fn oe_set(&self) {
        SIO.gpio_oe(self.bank()).value_set().write_value(self.mask())
    }

    #[inline]
    pub fn oe_clr(&self) {
        SIO.gpio_oe(self.bank()).value_clr().write_value(self.mask())
    }

    #[inline]
    pub fn out_set(&self) {
        SIO.gpio_out(self.bank()).value_set().write_value(self.mask())
    }

    #[inline]
    pub fn out_clr(&self) {
        SIO.gpio_out(self.bank()).value_clr().write_value(self.mask())
    }

    /// Get interrupt flags that are pending (even if not enabled)
    #[inline]
    pub fn interrupt_status(&self) -> EventMask {
        let reg = self.pin_in_bank() / 8;
        let offset = (self.pin_in_bank() % 8) * 4;
        EventMask(((self.io().intr(reg).read().0 >> offset) & 0x0f) as u8)
    }

    /// Enable interrupts to fire once.
    ///
    /// This is a low-level call that does not wait for the interrupt.
    #[inline]
    pub fn enable_interrupts(&self, mask: EventMask) {
        let reg = self.pin_in_bank() / 8;
        let offset = (self.pin_in_bank() % 8) * 4;
        self.io().int_proc(0).inte(reg).write_set(|w| {
            w.0 = (mask.0 as u32) << offset;
        });
    }

    /// Clear any pending edge interrupts
    pub fn clear_interrupts(&self) {
        let reg = self.pin_in_bank() / 8;
        let offset = (self.pin_in_bank() % 8) * 4;
        self.io().intr(reg).write(|w| {
            w.0 = 0x0f << offset;
        });
    }

    #[cfg(feature = "gpio-interrupts")]
    pub async fn wait(&self, rt: Runtime, mask: EventMask) -> EventMask {
        let int = BANK0_INT.get_pinned(rt);
        let reg = self.pin_in_bank() / 8;
        let offset = (self.pin_in_bank() % 8) * 4;

        int.until(||{
            let events = EventMask(((self.io().intr(reg).read().0 >> offset) & 0x0f) as u8);
            defmt::debug!("polling gpio{} events: {:b}", self.pin, events.0);
            if events.contains(mask) {
                Some(events)
            } else {
                // Enable requested interrupts
                self.io().int_proc(0).inte(reg).write_set(|w| {
                    w.0 = (mask.0 as u32) << offset;
                });
                None
            }
        }).await
    }

    #[inline]
    #[cfg(feature = "gpio-interrupts")]
    pub async fn wait_level(&self, rt: Runtime, high: bool) {
        self.wait(rt, if high { EventMask::HIGH } else { EventMask::LOW }).await;
    }
}

/// Type-level pin
pub trait TypePin {
    const DYN: IoPin;

    #[inline]
    fn set_function(func: Function) {
        Self::DYN.set_function(func)
    }

    #[inline]
    fn disable() {
        Self::DYN.disable()
    }

    #[inline]
    fn read() -> bool {
        Self::DYN.read()
    }

    #[inline]
    fn read_out() -> bool {
        Self::DYN.read_out()
    }

    #[inline]
    fn read_oe() -> bool {
        Self::DYN.read_oe()
    }

    #[inline]
    fn oe_set() {
        Self::DYN.oe_set()
    }

    #[inline]
    fn oe_clr() {
        Self::DYN.oe_clr()
    }

    #[inline]
    fn out_set() {
        Self::DYN.out_set()
    }

    #[inline]
    fn out_clr() {
        Self::DYN.out_clr()
    }
}

macro_rules! pins {
    ($($pin_id:ident = $pin_num:literal,)+) => {
        $(
            pub struct $pin_id {}
            impl TypePin for $pin_id {
                const DYN: IoPin = IoPin { pin: $pin_num };
            }
        )+
    };
}

pins! {
    GPIO00 = 0,
    GPIO01 = 1,
    GPIO02 = 2,
    GPIO03 = 3,
    GPIO04 = 4,
    GPIO05 = 5,
    GPIO06 = 6,
    GPIO07 = 7,
    GPIO08 = 8,
    GPIO09 = 9,
    GPIO10 = 10,
    GPIO11 = 11,
    GPIO12 = 12,
    GPIO13 = 13,
    GPIO14 = 14,
    GPIO15 = 15,
    GPIO16 = 16,
    GPIO17 = 17,
    GPIO18 = 18,
    GPIO19 = 19,
    GPIO20 = 20,
    GPIO21 = 21,
    GPIO22 = 22,
    GPIO23 = 23,
    GPIO24 = 24,
    GPIO25 = 25,
    GPIO26 = 26,
    GPIO27 = 27,
    GPIO28 = 28,
    GPIO29 = 29,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EventMask(u8);

impl EventMask {
    pub const NONE: Self = Self(0);
    pub const LOW: Self = Self(1);
    pub const HIGH: Self = Self(2);
    pub const FALLING: Self = Self(4);
    pub const RISING: Self = Self(8);
}

impl EventMask {
    pub fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn any(&self) -> bool {
        self.0 != 0
    }

    pub fn bits(&self) -> u8 {
        self.0
    }
}

impl core::ops::BitOr for EventMask {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

#[cfg(feature = "gpio-interrupts")]
pub static BANK0_INT: TaskOnly<InterruptList> = unsafe { TaskOnly::new_unsend(InterruptList::new()) };

#[cfg(feature = "gpio-interrupts")]
#[interrupt]
fn IO_IRQ_BANK0() {
    let io = crate::rp::pac::IO_BANK0;
    let wakers = unsafe { BANK0_INT.get_unchecked() };

    // Disable all interrupts before notifying tasks so a task can re-enable any it's still interested in
    let int_proc = io.int_proc(0);
    for reg in 0..4 {
        int_proc.inte(reg).write_clear(|w| {
            w.0 = !0;
        });
    }

    // SAFETY: This is an ISR at task priority
    unsafe { wakers.notify_all(); }
}
