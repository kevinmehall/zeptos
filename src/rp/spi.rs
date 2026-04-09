use core::slice;

use crate::rp::RpReg as _;
#[allow(unused_imports)]
use crate::{Interrupt, Runtime};
#[allow(unused_imports)]
use crate::rp::pac::{interrupt, resets, spi::Spi, RESETS};

pub trait StaticInstance: Instance {
    const ID: u8;

    unsafe fn steal() -> Self;
}

pub trait Instance {
    fn into_dyn(self) -> Dyn where Self:Sized + 'static {
        Dyn { regs: self.regs(), interrupt: self.interrupt() }
    }

    fn interrupt(&self) -> &'static Interrupt;

    fn regs(&self) -> Spi;

    fn id(&self) -> u8 {
        if self.regs() == rp_pac::SPI0 { 0 } else { 1 }
    }

    fn reset(&mut self) {
        RESETS.reset().write_value_set(reset_bit(self.id()));
    }

    fn unreset(&mut self) {
        let flags = reset_bit(self.id());
        RESETS.reset().write_value_clear(flags);
        while ((!RESETS.reset_done().read().0) & flags.0) != 0 {}
    }
}

fn reset_bit(id: u8) -> resets::regs::Peripherals {
    let mut p = resets::regs::Peripherals::default();
    if id == 0 {
        p.set_spi0(true)
    } else {
        p.set_spi1(true);
    }
    p
}

macro_rules! instance {
    ($feature:literal, $name:ident, $pac_name:ident, $irq:ident, $int:ident, $id:literal) => {
        #[cfg(feature = $feature)]
        pub struct $name(Runtime);

        #[cfg(feature = $feature)]
        static $int: crate::TaskOnly<Interrupt> = crate::TaskOnly::new(Interrupt::new());

        #[cfg(feature = $feature)]
        impl StaticInstance for $name {
            const ID: u8 = $id;

            /// ## Safety
            ///
            /// This must be called from within the runtime and the peripheral must not exist
            /// elsewhere in the program.
            unsafe fn steal() -> Self {
                unsafe { $name(Runtime::steal()) }
            }
        }

        #[cfg(feature = $feature)]
        impl Instance for $name {
            fn interrupt(&self) -> &'static Interrupt {
                $int.get(self.0)
            }

            fn regs(&self) -> Spi {
                rp_pac::$pac_name
            }
        }

        #[cfg(feature = $feature)]
        #[interrupt]
        fn $irq() {
            rp_pac::$pac_name.imsc().write(|_| { });
            unsafe { $int.get_unchecked().notify() };
        }
    };
}

instance!("spi0", Spi0, SPI0, SPI0_IRQ, SPI0_INT, 0);
instance!("spi1", Spi1, SPI1, SPI1_IRQ, SPI1_INT, 1);

pub struct Dyn {
    regs: Spi,
    interrupt: &'static Interrupt,
}

impl Instance for Dyn {
    fn interrupt(&self) -> &'static Interrupt {
        self.interrupt
    }

    fn regs(&self) -> Spi {
        self.regs
    }
}

impl<T> Instance for &mut T where T: Instance {
    fn interrupt(&self) -> &'static Interrupt {
        (**self).interrupt()
    }

    fn regs(&self) -> Spi {
        (**self).regs()
    }
}

#[non_exhaustive]
pub struct Config {
    pub mode: u8,
    pub cpsdvsr: u8,
    pub scr: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: 0,
            cpsdvsr: 2,
            scr: 0,
        }
    }
}

impl Config {
    pub const BASE_CLOCK_HZ: u32 = crate::rp::CLK_PERI_HZ / 2;
    pub const MAX_DIV: u32 = 127 * 256;

    pub fn get_divisor(&self) -> u32 {
        self.cpsdvsr as u32 * (self.scr as u32 + 1)
    }

    pub fn get_rate(&self) -> u32 {
        crate::rp::CLK_PERI_HZ / self.get_divisor()
    }

    pub fn set_rate(&mut self, rate: u32) -> Result<(), ()> {
        if rate == 0 {
            return Err(());
        }

        self.set_divisor(Self::BASE_CLOCK_HZ.div_ceil(rate))
    }

    pub fn set_divisor(&mut self, divisor: u32) -> Result<(), ()> {
        if divisor == 0 || divisor > Self::MAX_DIV {
            return Err(());
        }

        let cpsdvsr_over_2 = divisor.div_ceil(256);
        self.cpsdvsr = (cpsdvsr_over_2 * 2) as u8;
        self.scr = if cpsdvsr_over_2 == 1 {
            divisor as u8
        } else {
            (divisor.div_ceil(cpsdvsr_over_2) - 1) as u8
        };

        Ok(())
    }
}

pub struct Controller<I: Instance> {
    instance: I,
}

pub trait Dest {
    fn put(&mut self, byte: u8);
}

pub trait IntoDest {
    type Dest: Dest;
    fn into_dest(self) -> Self::Dest;
}

impl Dest for () {
    fn put(&mut self, _byte: u8) {}
}

impl IntoDest for () {
    type Dest = ();
    fn into_dest(self) -> Self::Dest {}
}

impl<'a> Dest for slice::IterMut<'a, u8> {
    fn put(&mut self, byte: u8) {
        if let Some(slot) = self.next() {
            *slot = byte;
        }
    }
}

impl<'a> IntoDest for &'a mut [u8] {
    type Dest = slice::IterMut<'a, u8>;
    fn into_dest(self) -> Self::Dest {
        self.iter_mut()
    }
}

fn set_config(regs: Spi, config: Config) {
    regs.cr1().write(|w| w.set_sse(false));
    regs.cpsr().write(|w| w.set_cpsdvsr(config.cpsdvsr));
    regs.cr0().write(|w| {
        w.set_dss(0b0111); // 8bit
        w.set_sph(config.mode & 0b01 != 0);
        w.set_spo(config.mode & 0b10 != 0);
        w.set_scr(config.scr);
    });
    regs.cr1().write(|w| w.set_sse(true));
}

impl<I: Instance> Controller<I> {
    pub fn new(mut instance: I, config: Config) -> Self {
        instance.reset();
        instance.unreset();
        set_config(instance.regs(), config);
        Self { instance }
    }

    pub fn set_config(&mut self, config: Config) {
        set_config(self.instance.regs(), config);
    }

    pub fn transfer<S, D>(&mut self, src: S, dest: D) -> impl Future<Output = ()> where S: IntoIterator<Item = u8>, S::IntoIter: Unpin, D: IntoDest, D::Dest: Unpin {
        async fn transfer(regs: Spi, interrupt: &Interrupt, mut src: impl Iterator<Item = u8> + Unpin, mut dest: impl Dest + Unpin) {
            let mut tx_done: bool = false;
            let mut pending: u8 = 0;

            interrupt.until(move || {
                let stat = regs.sr().read();

                if !tx_done && stat.tnf() {
                    if let Some(byte) = src.next() {
                        regs.dr().write(|w| w.set_data(byte as u16));
                        pending += 1;
                    } else {
                        tx_done = true;
                    }
                }

                if pending > 0 && stat.rne() {
                    let data = regs.dr().read().data() as u8;
                    dest.put(data);
                    pending -= 1;
                }

                regs.imsc().write(|w| {
                    w.set_txim(!tx_done);
                    w.set_rxim(pending > 0);
                    w.set_rtim(pending > 0);
                });

                tx_done && pending == 0
            }).await
        }
        transfer(self.instance.regs(), self.instance.interrupt(), src.into_iter(), dest.into_dest())
    }
}

impl<I: Instance> Drop for Controller<I> {
    fn drop(&mut self) {
        defmt::debug!("Dropping SPI");
        self.instance.reset();
    }
}
