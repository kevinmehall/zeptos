use defmt::panic;

use crate::rp::RpReg as _;
#[allow(unused_imports)]
use crate::{Interrupt, Runtime};
#[allow(unused_imports)]
use crate::rp::pac::{interrupt, resets, i2c::I2c, RESETS};

pub trait StaticInstance: Instance {
    const ID: u8;

    unsafe fn steal() -> Self;
}

pub trait Instance {
    fn into_dyn(self) -> Dyn where Self:Sized + 'static {
        Dyn { regs: self.regs(), interrupt: self.interrupt() }
    }

    fn interrupt(&self) -> &'static Interrupt;

    fn regs(&self) -> I2c;

    fn id(&self) -> u8 {
        if self.regs() == rp_pac::I2C0 { 0 } else { 1 }
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
        p.set_i2c0(true)
    } else {
        p.set_i2c1(true);
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

            fn regs(&self) -> I2c {
                rp_pac::$pac_name
            }
        }

        #[cfg(feature = $feature)]
        #[interrupt]
        fn $irq() {
            rp_pac::$pac_name.ic_intr_mask().write(|_| { });
            unsafe { $int.get_unchecked().notify() };
        }
    };
}

instance!("i2c0", I2c0, I2C0, I2C0_IRQ, I2C0_INT, 0);
instance!("i2c1", I2c1, I2C1, I2C1_IRQ, I2C1_INT, 1);

pub struct Dyn {
    regs: I2c,
    interrupt: &'static Interrupt,
}

impl Instance for Dyn {
    fn interrupt(&self) -> &'static Interrupt {
        self.interrupt
    }

    fn regs(&self) -> I2c {
        self.regs
    }
}

impl<T> Instance for &mut T where T: Instance {
    fn interrupt(&self) -> &'static Interrupt {
        (**self).interrupt()
    }

    fn regs(&self) -> I2c {
        (**self).regs()
    }
}

#[non_exhaustive]
pub struct Config {
    pub frequency: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            frequency: 100_000,
        }
    }
}

pub struct Controller<I: Instance> {
    instance: I,
}

impl<I: Instance> Controller<I> {
    pub fn new(mut instance: I, config: Config) -> Self {
        fn init(regs: I2c, config: Config) {
            let clk_base = crate::CLOCK_HZ;
            let period = (clk_base + config.frequency / 2) / config.frequency;
            let lcnt = period * 3 / 5; // spend 3/5 (60%) of the period low
            let hcnt = period - lcnt; // and 2/5 (40%) of the period high

            if hcnt > 0xffff || lcnt > 0xffff {
                panic!("clock too fast");
            }
            if hcnt < 8 || lcnt < 8 {
                panic!("clock too slow");
            }

            // Per I2C-bus specification a device in standard or fast mode must
            // internally provide a hold time of at least 300ns for the SDA
            // signal to bridge the undefined region of the falling edge of SCL.
            // A smaller hold time of 120ns is used for fast mode plus.
            let sda_tx_hold_count = if config.frequency < 1_000_000 {
                // sda_tx_hold_count = clk_base [cycles/s] * 300ns * (1s /
                // 1e9ns) Reduce 300/1e9 to 3/1e7 to avoid numbers that don't
                // fit in uint. Add 1 to avoid division truncation.
                ((clk_base * 3) / 10_000_000) + 1
            } else {
                // fast mode plus requires a clk_base > 32MHz
                if clk_base <= 32_000_000 {
                    panic!("clock too fast");
                }

                // sda_tx_hold_count = clk_base [cycles/s] * 120ns * (1s /
                // 1e9ns) Reduce 120/1e9 to 3/25e6 to avoid numbers that don't
                // fit in uint. Add 1 to avoid division truncation.
                ((clk_base * 3) / 25_000_000) + 1
            };

            if sda_tx_hold_count > lcnt - 2 {
                panic!("clock too slow");
            }

            regs.ic_enable().write(|w| { w.set_enable(false); });

            regs.ic_con().write(|w| {
                w.set_master_mode(false);
                w.set_ic_slave_disable(true);
            });

            regs.ic_fs_scl_hcnt().write(|w| w.set_ic_fs_scl_hcnt(hcnt as u16));
            regs.ic_fs_scl_lcnt().write(|w| w.set_ic_fs_scl_lcnt(lcnt as u16));
            regs.ic_ss_scl_hcnt().write(|w| w.set_ic_ss_scl_hcnt(hcnt as u16));
            regs.ic_ss_scl_lcnt().write(|w| w.set_ic_ss_scl_lcnt(lcnt as u16));
            regs.ic_fs_spklen()
                .write(|w| w.set_ic_fs_spklen(if lcnt < 16 { 1 } else { (lcnt / 16) as u8 }));
            regs.ic_sda_hold()
                .modify(|w| w.set_ic_sda_tx_hold(sda_tx_hold_count as u16));

            regs.ic_con().write(|w| {
                w.set_master_mode(true);
                w.set_ic_slave_disable(true);
                w.set_ic_restart_en(true);
                w.set_tx_empty_ctrl(true);
                w.set_speed(if config.frequency <= 100_000 {
                    rp_pac::i2c::vals::Speed::STANDARD
                } else if config.frequency <= 1_000_000 {
                    rp_pac::i2c::vals::Speed::FAST
                } else {
                    rp_pac::i2c::vals::Speed::HIGH
                });
            });
        }

        instance.reset();
        instance.unreset();
        init(instance.regs(), config);
        Self { instance }
    }

    pub fn set_address(&mut self, addr: u16) {
        fn set_address(regs: I2c, addr: u16) {
            regs.ic_enable().write(|w| w.set_enable(false));
            regs.ic_tar().write(|w| w.set_ic_tar(addr));
            regs.ic_enable().write(|w| w.set_enable(true));
        }
        set_address(self.instance.regs(), addr);
    }

    pub fn get_address(&mut self) -> u16 {
        self.instance.regs().ic_tar().read().ic_tar()
    }

    pub fn write(&mut self, byte: u8, restart: bool, stop: bool) -> impl Future<Output = Result<(), Error>> {
        async fn write(regs: I2c, interrupt: &Interrupt, byte: u8, restart: bool, stop: bool) -> Result<(), Error> {
            regs.ic_data_cmd().write(|w| {
                w.set_restart(restart);
                w.set_stop(stop);
                w.set_cmd(false);
                w.set_dat(byte);
            });

            interrupt.until(|| {
                let reg = regs.ic_raw_intr_stat().read();
                defmt::debug!("Polling write: {:032b}", reg.0);
                if reg.tx_abrt() {
                    Some(Err(Error::from_reg(regs.ic_tx_abrt_source().read())))
                } else if reg.tx_empty() {
                    Some(Ok(()))
                } else {
                    regs.ic_intr_mask().write(|w| {
                        w.set_m_tx_empty(true);
                        w.set_m_tx_abrt(true);
                    });
                    None
                }
            }).await
        }
        write(self.instance.regs(), self.instance.interrupt(), byte, restart, stop)
    }

    pub fn read(&mut self, restart: bool, stop: bool) -> impl Future<Output = Result<u8, Error>> {
        async fn read(regs: I2c, interrupt: &Interrupt, restart: bool, stop: bool) -> Result<u8, Error> {
            regs.ic_data_cmd().write(|w| {
                w.set_restart(restart);
                w.set_stop(stop);
                w.set_cmd(true);
            });

            interrupt.until(|| {
                let reg =  regs.ic_raw_intr_stat().read();
                if reg.rx_full() {
                    let data = regs.ic_data_cmd().read().dat();
                    Some(Ok(data))
                } else if reg.tx_abrt() {
                    Some(Err(Error::from_reg(regs.ic_tx_abrt_source().read())))
                } else {
                    regs.ic_intr_mask().write(|w| {
                        w.set_m_tx_empty(true);
                        w.set_m_tx_abrt(true);
                    });
                    None
                }
            }).await
        }
        read(self.instance.regs(), self.instance.interrupt(), restart, stop)
    }

    pub fn abort(&mut self) -> impl Future<Output = ()> {
        async fn abort(regs: I2c, interrupt: &Interrupt) {
            regs.ic_enable().write(|w| { w.set_enable(true); w.set_abort(true) });
            interrupt.until(|| {
                if regs.ic_raw_intr_stat().read().tx_abrt() {
                    true
                } else {
                    regs.ic_intr_mask().write(|w| w.set_m_tx_abrt(true));
                    false
                }
            }).await;
            regs.ic_clr_tx_abrt().read();
            regs.ic_enable().write(|w| { w.set_enable(false); });
        }
        abort(self.instance.regs(), self.instance.interrupt())
    }
}

impl<I: Instance> Drop for Controller<I> {
    fn drop(&mut self) {
        defmt::debug!("Dropping I2C");
        self.instance.reset();
    }
}


#[derive(defmt::Format)]
pub enum Error {
    AddrNack,
    DataNack,
    ArbitrationLost,
    Unknown,
}

impl Error {
    fn from_reg(src: rp_pac::i2c::regs::IcTxAbrtSource) -> Self {
        if src.arb_lost() {
            Error::ArbitrationLost
        } else if src.abrt_txdata_noack() {
            Error::DataNack
        } else if src.abrt_10addr1_noack() || src.abrt_10addr2_noack() || src.abrt_7b_addr_noack() {
            Error::AddrNack
        } else {
            defmt::error!("Unknown I2C error {:032b}", src.0);
            Error::Unknown
        }
    }
}
