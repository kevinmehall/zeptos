use crate::samd::pac::{PORT, PORT_IOBUS};
use crate::samd::pac::port::{
    CTRL, DIR, DIRCLR, DIRSET, DIRTGL, IN, OUT, OUTCLR, OUTSET, OUTTGL, PINCFG0_ as PINCFG,
    PMUX0_ as PMUX, WRCONFIG,
};

/// The [`PORT`] register block
#[repr(C)]
#[allow(clippy::upper_case_acronyms)]
struct GROUP {
    dir: DIR,
    dirclr: DIRCLR,
    dirset: DIRSET,
    dirtgl: DIRTGL,
    out: OUT,
    outclr: OUTCLR,
    outset: OUTSET,
    outtgl: OUTTGL,
    in_: IN,
    ctrl: CTRL,
    wrconfig: WRCONFIG,
    _padding1: [u8; 4],
    pmux: [PMUX; 16],
    pincfg: [PINCFG; 32],
    _padding2: [u8; 32],
}

pub trait AlternateFunc {
    const DYN: Alternate;
}

macro_rules! alternate {
    ($($letter:ident),+) => {
        #[repr(u8)]
        pub enum Alternate { $($letter),+ }
        pub mod alternate {
            use super::{Alternate, AlternateFunc};
            $(
                pub enum $letter {}
                impl AlternateFunc for $letter {
                    const DYN: Alternate = Alternate::$letter;
                }
            )+
        }
    };
}
alternate!(A, B, C, D, E, F, G, H);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct IoPin {
    pub group: u8,
    pub pin: u8,
}

impl IoPin {
    #[inline]
    fn group(&self) -> &'static GROUP {
        const GROUPS: *const GROUP = PORT::ptr() as *const _;
        unsafe { &*GROUPS.add(self.group as usize) }
    }

    #[inline]
    fn group_iobus(&self) -> &'static GROUP {
        const GROUPS: *const GROUP = PORT_IOBUS::ptr() as *const _;
        unsafe { &*GROUPS.add(self.group as usize) }
    }

    #[inline]
    fn mask_32(&self) -> u32 {
        1 << self.pin as u32
    }

    #[inline]
    fn mask_16(&self) -> u16 {
        1 << (self.pin & 0xF)
    }

    #[inline]
    fn hwsel(&self) -> bool {
         self.pin & 0x10 != 0
    }

    #[inline]
    pub fn enable_sampling(&self) {
        unsafe {
            self.group().ctrl.write(|w| w.bits(0xffffffff))
        }
    }

    #[inline]
    pub fn pincfg(&self) -> &'static PINCFG {
        &self.group().pincfg[self.pin as usize]
    }

    #[inline]
    pub fn set_alternate(&self, pmux: Alternate) {
        self.group().wrconfig.write(|w| {
            w.hwsel().bit(self.hwsel());
            w.wrpincfg().set_bit();
            w.wrpmux().set_bit();
            w.pmux().variant(pmux as u8);
            w.pmuxen().bit(true);
            w.pinmask().variant(self.mask_16())
        });
    }

    #[inline]
    pub fn set_io(&self) {
        self.group().wrconfig.write(|w| {
            w.hwsel().bit(self.hwsel());
            w.wrpincfg().set_bit();
            w.wrpmux().set_bit();
            w.pmux().variant(0);
            w.pmuxen().bit(false);
            w.pinmask().variant(self.mask_16())
        });
    }

    #[inline]
    pub fn read(&self) -> bool {
        let mask = self.mask_32();
        self.group().in_.read().bits() & mask != 0
    }

    #[inline]
    pub fn read_iobus(&self) -> bool {
        let mask = self.mask_32();
        self.group_iobus().in_.read().bits() & mask != 0
    }

    #[inline]
    pub fn read_out(&self) -> bool {
        let mask = self.mask_32();
        self.group_iobus().out.read().bits() & mask != 0
    }

    #[inline]
    pub fn read_dir(&self) -> bool {
        let mask = self.mask_32();
        self.group_iobus().dir.read().bits() & mask != 0
    }

    #[inline]
    pub fn outset(&self) {
        unsafe {
            self.group_iobus().outset.write(|w| w.bits(self.mask_32()));
        }
    }

    #[inline]
    pub fn outclr(&self) {
        unsafe {
            self.group_iobus().outclr.write(|w| w.bits(self.mask_32()));
        }
    }

    #[inline]
    pub fn outtgl(&self) {
        unsafe {
            self.group_iobus().outtgl.write(|w| w.bits(self.mask_32()));
        }
    }

    #[inline]
    pub fn dirset(&self) {
        unsafe {
            self.group_iobus().dirset.write(|w| w.bits(self.mask_32()));
        }
    }

    #[inline]
    pub fn dirclr(&self) {
        unsafe {
            self.group_iobus().dirclr.write(|w| w.bits(self.mask_32()));
        }
    }

    #[inline]
    pub fn dirtgl(&self) {
        unsafe {
            self.group_iobus().dirtgl.write(|w| w.bits(self.mask_32()));
        }
    }
}

/// Type-level pin
pub trait TypePin {
    const DYN: IoPin;

    #[inline]
    fn enable_sampling() {
        Self::DYN.enable_sampling()
    }

    #[inline]
    fn pincfg() -> &'static PINCFG {
        Self::DYN.pincfg()
    }

    #[inline]
    fn set_alternate(pmux: Alternate) {
        Self::DYN.set_alternate(pmux)
    }

    #[inline]
    fn set_io() {
        Self::DYN.set_io()
    }

    #[inline]
    fn read() -> bool {
        Self::DYN.read_iobus()
    }

    #[inline]
    fn read_iobus() -> bool {
        Self::DYN.read_iobus()
    }

    #[inline]
    fn read_out() -> bool {
        Self::DYN.read_out()
    }

    #[inline]
    fn read_dir() -> bool {
        Self::DYN.read_dir()
    }

    #[inline]
    fn outset() {
        Self::DYN.outset()
    }

    #[inline]
    fn outclr() {
        Self::DYN.outclr()
    }

    #[inline]
    fn outtgl() {
        Self::DYN.outtgl()
    }

    #[inline]
    fn dirset() {
        Self::DYN.dirset()
    }

    #[inline]
    fn dirclr() {
        Self::DYN.dirclr()
    }

    #[inline]
    fn dirtgl() {
        Self::DYN.dirtgl()
    }
}

macro_rules! pins {
    ($($group:ident = $group_num:literal { $($pin_id:ident = $pin_num:literal,)+ } ),+) => {
        $(
            $(
                pub struct $pin_id {}
                impl TypePin for $pin_id {
                    const DYN: IoPin = IoPin { group: $group_num, pin: $pin_num };
                }
            )+
            
        )+
    };
}

pins! {
    A = 0 {
        PA00 = 0,
        PA01 = 1,
        PA02 = 2,
        PA03 = 3,
        PA04 = 4,
        PA05 = 5,
        PA06 = 6,
        PA07 = 7,
        PA08 = 8,
        PA09 = 9,
        PA10 = 10,
        PA11 = 11,
        PA12 = 12,
        PA13 = 13,
        PA14 = 14,
        PA15 = 15,
        PA16 = 16,
        PA17 = 17,
        PA18 = 18,
        PA19 = 19,
        PA20 = 20,
        PA21 = 21,
        PA22 = 22,
        PA23 = 23,
        PA24 = 24,
        PA25 = 25,
        PA26 = 26,
        PA27 = 27,
        PA28 = 28,
        PA29 = 29,
        PA30 = 30,
        PA31 = 31,
    },
    B = 1 {
        PB00 = 0,
        PB01 = 1,
        PB02 = 2,
        PB03 = 3,
        PB04 = 4,
        PB05 = 5,
        PB06 = 6,
        PB07 = 7,
        PB08 = 8,
        PB09 = 9,
        PB10 = 10,
        PB11 = 11,
        PB12 = 12,
        PB13 = 13,
        PB14 = 14,
        PB15 = 15,
        PB16 = 16,
        PB17 = 17,
        PB18 = 18,
        PB19 = 19,
        PB20 = 20,
        PB21 = 21,
        PB22 = 22,
        PB23 = 23,
        PB24 = 24,
        PB25 = 25,
        PB26 = 26,
        PB27 = 27,
        PB28 = 28,
        PB29 = 29,
        PB30 = 30,
        PB31 = 31,
    },
    C = 2 {
        PC00 = 0,
        PC01 = 1,
        PC02 = 2,
        PC03 = 3,
        PC04 = 4,
        PC05 = 5,
        PC06 = 6,
        PC07 = 7,
        PC08 = 8,
        PC09 = 9,
        PC10 = 10,
        PC11 = 11,
        PC12 = 12,
        PC13 = 13,
        PC14 = 14,
        PC15 = 15,
        PC16 = 16,
        PC17 = 17,
        PC18 = 18,
        PC19 = 19,
        PC20 = 20,
        PC21 = 21,
        PC22 = 22,
        PC23 = 23,
        PC24 = 24,
        PC25 = 25,
        PC26 = 26,
        PC27 = 27,
        PC28 = 28,
        PC29 = 29,
        PC30 = 30,
        PC31 = 31,
    },
    D = 3 {
        PD00 = 0,
        PD01 = 1,
        PD02 = 2,
        PD03 = 3,
        PD04 = 4,
        PD05 = 5,
        PD06 = 6,
        PD07 = 7,
        PD08 = 8,
        PD09 = 9,
        PD10 = 10,
        PD11 = 11,
        PD12 = 12,
        PD13 = 13,
        PD14 = 14,
        PD15 = 15,
        PD16 = 16,
        PD17 = 17,
        PD18 = 18,
        PD19 = 19,
        PD20 = 20,
        PD21 = 21,
        PD22 = 22,
        PD23 = 23,
        PD24 = 24,
        PD25 = 25,
        PD26 = 26,
        PD27 = 27,
        PD28 = 28,
        PD29 = 29,
        PD30 = 30,
        PD31 = 31,
    }
}
