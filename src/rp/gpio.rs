use rp_pac::{common::{Reg, RW}, io::Io, SIO};

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
    fn bank(&self) -> usize {
        (self.pin >> 5) as usize
    }

    fn pin_in_bank(&self) -> usize {
        (self.pin & 0x1f) as usize
    }

    fn mask(&self) -> u32 {
        1 << self.pin_in_bank()
    }

    #[inline]
    fn pads(&self) -> Reg<rp_pac::pads::regs::GpioCtrl, RW> {
        if self.pin <= 31 {
            crate::rp::pac::PADS_BANK0.gpio(self.pin_in_bank())
        } else {
            crate::rp::pac::PADS_QSPI.gpio(self.pin_in_bank())
        }
    }

    #[inline]
    fn io(&self) -> Io {
        if self.pin <= 31 {
            crate::rp::pac::IO_BANK0
        } else {
            crate::rp::pac::IO_QSPI
        }
    }

    #[inline]
    pub fn set_function(&self, func: Function) {
        self.io().gpio(self.pin_in_bank()).ctrl().write(|w| {
            w.set_funcsel(func as u8);
        });
    }

    #[inline]
    pub fn disable(&self) {
        self.io().gpio(self.pin_in_bank()).ctrl().write(|w| {
            w.set_funcsel(31);
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
    GPIO30 = 30,
}

