use crate::Interrupt;

use super::{ Sercom, RegisterBlock };

#[derive(Copy, Clone)]
pub struct SpiConfig {
    pub dopo: u8,
    pub dipo: u8,
    pub clkdiv_minus_one: u8,
    pub mode: u8,
}

impl SpiConfig {
    pub const BASE_CLOCK: u32 = crate::CLOCK_HZ / 2;
}

impl Default for SpiConfig {
    fn default() -> Self {
        SpiConfig {
            dopo: 0,
            dipo: 0,
            clkdiv_minus_one: (Self::BASE_CLOCK / 1_000_000 - 1) as u8, // 1MHz default
            mode: 0,
        }
    }
}

pub struct SpiController<S: Sercom> {
    sercom: S,
}

impl<S: Sercom> SpiController<S> {
    pub fn new(sercom: S, config: SpiConfig) -> Self {
        init(sercom.regs(), config);
        Self { sercom }
    }

    pub fn transfer(&mut self, out: u8) -> impl Future<Output = u8> {
        transfer_byte(self.sercom.regs(), self.sercom.interrupt(), out)
    }
}

impl <S: Sercom> Drop for SpiController<S> {
    fn drop(&mut self) {
        deinit(self.sercom.regs());
    }
}

fn init(regs: &RegisterBlock, config: SpiConfig) {
    let regs = regs.spi();
    regs.ctrla.write(|w| w.mode().spi_master());
    regs.baud.write(|w| w.baud().variant(config.clkdiv_minus_one));
    regs.ctrlb.write(|w| {
        w.rxen().set_bit()
    });
    regs.ctrla.write(|w| {
        w.mode().spi_master();
        w.dopo().variant(config.dopo);
        w.dipo().variant(config.dipo);
        w.cpha().variant(config.mode & 0b01 != 0);
        w.cpol().variant(config.mode & 0b10 != 0);
        w.enable().set_bit()
    });
    while regs.syncbusy.read().enable().bit_is_set() {}
}

async fn transfer_byte(regs: &RegisterBlock, interrupt: &Interrupt, out: u8) -> u8 {
    let regs = regs.spi();
    regs.data.write(|w| w.data().variant(out as u16));

    interrupt.until(|| {
        if regs.intflag.read().txc().bit_is_set() {
            true
        } else {
            regs.intenset.write(|w| { w.txc().set_bit() });
            false
        }
    }).await;

    regs.data.read().data().bits() as u8
}

fn deinit(regs: &RegisterBlock) {
    regs.spi().ctrla.write(|w| w.swrst().set_bit());
}
