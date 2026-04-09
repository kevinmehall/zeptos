use crate::{Interrupt, samd::pac::sercom0::I2CM};

use super::Sercom;

pub struct I2cController<S: Sercom> {
    sercom: S,
}

pub enum I2cError {
    ArbitrationLost,
    NoAcknowledge,
}

impl<S: Sercom> I2cController<S> {
    pub fn new(sercom: S) -> Self {
        init(sercom.regs().i2cm());
        Self { sercom }
    }

    /// Send a start condition and the address byte with the R/W bit.
    ///
    /// If a read, the first byte will be received.
    pub fn start(&mut self, addr: u8) -> impl Future<Output = Result<(), I2cError>> {
        start(self.sercom.regs().i2cm(), self.sercom.interrupt(), addr)
    }

    /// Read the first data byte after a start with the RW bit set.
    pub fn read_first(&mut self) -> u8 {
        self.sercom.regs().i2cm().data.read().data().bits() as u8
    }

    /// Ack the previous byte, and read and return the next.
    pub async fn read_next(&mut self) -> Result<u8, I2cError> {
        ack_read(self.sercom.regs().i2cm(), self.sercom.interrupt()).await?;
        Ok(self.read_first())
    }

    /// Write a data byte.
    pub fn write(&mut self, data: u8) -> impl Future<Output = Result<(), I2cError>> {
        write(self.sercom.regs().i2cm(), self.sercom.interrupt(), data)
    }

    /// Send a stop condition.
    pub fn stop(&mut self) {
        stop(self.sercom.regs().i2cm())
    }
}

impl<S: Sercom> Drop for I2cController<S> {
    fn drop(&mut self) {
        deinit(self.sercom.regs().i2cm());
    }
}

fn init(regs: &I2CM) {
    regs.ctrla.write(|w| w.mode().i2c_master());
    regs.baud.write(|w| w.baud().variant(235) ); // 100kHz
    regs.ctrla.write(|w|
        w.mode().i2c_master()
            .enable().set_bit()
    );
    while regs.syncbusy.read().enable().bit_is_set() {}

    regs.status.write(|w| w.busstate().variant(1) ); // set idle
    sync_sysop(regs);
}

fn deinit(regs: &I2CM) {
    regs.ctrla.write(|w| w.swrst().set_bit());
}

fn wait(regs: &I2CM, interrupt: &Interrupt) -> impl Future<Output = Result<(), I2cError>> {
    interrupt.until(move || {
        let flags = regs.intflag.read();

        if flags.mb().bit_is_set() | flags.sb().bit_is_set() {
            let stat = regs.status.read();
            if stat.arblost().bit_is_set() {
                return Some(Err(I2cError::ArbitrationLost));
            } else if stat.rxnack().bit_is_set() {
                return Some(Err(I2cError::NoAcknowledge));
            } else {
                return Some(Ok(()));
            }
        }

        regs.intenset.write(|w| {
            w.mb().set_bit();
            w.sb().set_bit();
            w
        });

        None
    })
}

async fn start(regs: &I2CM, interrupt: &Interrupt, addr: u8) -> Result<(), I2cError> {
    regs.addr.write(|w| w.addr().variant(addr as u16));
    sync_sysop(regs);
    wait(regs, interrupt).await
}

async fn write(regs: &I2CM, interrupt: &Interrupt, data: u8) -> Result<(), I2cError> {
    regs.data.write(|w| w.data().variant(data));
    sync_sysop(regs);
    wait(regs, interrupt).await
}

async fn ack_read(regs: &I2CM, interrupt: &Interrupt) -> Result<(), I2cError> {
    // Ack previous byte, read the next
    regs.ctrlb.write(|w| w.cmd().variant(0x02));
    sync_sysop(regs);
    wait(regs, interrupt).await
}

fn stop(regs: &I2CM) {
    regs.ctrlb.write(|w| {
        w.ackact().set_bit(); // send nack if read
        w.cmd().variant(0x3)
    });
    sync_sysop(regs);
}

#[inline]
fn sync_sysop(regs: &I2CM) {
    while regs.syncbusy.read().sysop().bit_is_set() {}
}
