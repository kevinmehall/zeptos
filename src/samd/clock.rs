//! Clock configuration
//! 
//! Based on atsamd-rs under MIT OR Apache-2.0.

use crate::samd::pac::gclk::clkctrl::GENSELECT_A::{self, *};
use crate::samd::pac::gclk::clkctrl::IDSELECT_A::{self, *};
use crate::samd::pac::gclk::genctrl::SRCSELECT_A::{self, *};
use crate::samd::pac::{GCLK, NVMCTRL, PM, SYSCTRL};

pub fn set_gclk_divider_and_source(
    gclk: &mut GCLK,
    gclk_id: GENSELECT_A,
    divider: u16,
    src: SRCSELECT_A,
    improve_duty_cycle: bool,
) {
    gclk.gendiv.write(|w| unsafe {
        w.id().bits(u8::from(gclk_id));
        w.div().bits(divider)
    });

    wait_for_sync(gclk);

    gclk.genctrl.write(|w| unsafe {
        w.id().bits(u8::from(gclk_id));
        w.src().bits(u8::from(src));
        // divide directly by divider, rather than exponential
        w.divsel().clear_bit();
        w.idc().bit(improve_duty_cycle);
        w.genen().set_bit();
        w.oe().set_bit()
    });

    wait_for_sync(gclk);
}

pub fn enable_clock(gclk: &mut GCLK, clock: IDSELECT_A, generator: GENSELECT_A) {
    gclk.clkctrl.write(|w| unsafe {
        w.id().bits(u8::from(clock));
        w.gen().bits(u8::from(generator));
        w.clken().set_bit()
    });
    wait_for_sync(gclk);
}

fn wait_for_sync(gclk: &mut GCLK) {
    while gclk.status.read().syncbusy().bit_is_set() {}
}

#[cfg(any(
    feature="samd-clock-48m-usb",
    feature="samd-clock-48m-internal",
    feature="samd-clock-48m-external-32k-osc",
    feature="samd-clock-48m-external-32k-xtal",
))]
pub(crate) fn configure_clocks() {
    let gclk = &mut unsafe { GCLK::steal() };
    let pm = &mut unsafe { PM::steal() };
    let sysctrl = unsafe { SYSCTRL::steal() };
    let nvmctrl = unsafe { NVMCTRL::steal() };

    // Set flash to 1 wait state for operation at 3.3V at 48MHz.
    nvmctrl.ctrlb.write(|w| {
        w.rws().half();
        w.manw().set_bit();
        w
    });

    let src = if cfg!(any(feature = "samd-clock-48m-external-32k-osc", feature = "samd-clock-48m-external-32k-xtal")) {
        // Configure external 32KHz oscillator input
        sysctrl.xosc32k.write(|w| {
            unsafe { w.startup().bits(6); }
            w.ondemand().clear_bit();
            w.runstdby().set_bit();
            w.en32k().set_bit();
            w.xtalen().bit(cfg!(feature = "samd-clock-48m-external-32k-xtal"));
            w
        });

        sysctrl.xosc32k.modify(|_, w| w.enable().set_bit());
        while sysctrl.pclksr.read().xosc32krdy().bit_is_clear() {}

        XOSC32K
    } else {
        let calibration = super::calibration::osc32k_cal();
        sysctrl.osc32k.write(|w| {
            unsafe {
                w.ondemand().clear_bit();
                w.calib().bits(calibration);
                // 6 here means: use 66 cycles of OSC32k to start up this oscillator
                w.startup().bits(6);
            }
            w.en32k().set_bit();
            w.enable().set_bit();
            w.runstdby().set_bit()
        });

        // Wait for the oscillator to stabilize
        while sysctrl.pclksr.read().osc32krdy().bit_is_clear() {}

        OSC32K
    };

    set_gclk_divider_and_source(gclk, GCLK1, 1, src, false);

    // Feed 32khz into the DFLL48
    enable_clock(gclk, DFLL48, GCLK1);

    // Turn it off while we configure it.
    // Note that we need to turn off on-demand mode and
    // disable it here, rather than just reseting the ctrl
    // register, otherwise our configuration attempt fails.
    sysctrl.dfllctrl.write(|w| w.ondemand().clear_bit());

    while sysctrl.pclksr.read().dfllrdy().bit_is_clear() {}

    // Apply calibration
    let coarse = super::calibration::dfll48m_coarse_cal();
    let fine = 0x1ff;

    sysctrl.dfllval.write(|w| unsafe {
        w.coarse().bits(coarse);
        w.fine().bits(fine)
    });

    if cfg!(feature = "samd-clock-48m-usb") {
        sysctrl.dfllmul.write(|w| unsafe {
            w.cstep().bits(1);
            w.fstep().bits(10);
            // scaling factor for 1 kHz USB SOF signal
            w.mul().bits((48_000_000u32 / 1000) as u16)
        }); 
    } else {
        sysctrl.dfllmul.write(|w| unsafe {
            w.cstep().bits(31);
            w.fstep().bits(511);
            // scaling factor between the clocks
            w.mul().bits(((48_000_000u32 + 32768 / 2) / 32768) as u16)
        });
    }

    // Turn it on
    sysctrl.dfllctrl.write(|w| {
        // always on
        w.ondemand().clear_bit();

        // closed loop mode
        w.mode().set_bit();

        // chill cycle disable
        w.ccdis().bit(cfg!(feature = "samd-clock-48m-usb"));

        // usb correction
        w.usbcrm().bit(cfg!(feature = "samd-clock-48m-usb"));

        // bypass coarse lock (have calibration data)
        w.bplckc().set_bit()
    });

    while sysctrl.pclksr.read().dfllrdy().bit_is_clear() {}

    // and finally enable it!
    sysctrl.dfllctrl.modify(|_, w| w.enable().set_bit());

    if cfg!(not(feature = "samd-clock-48m-usb")) {
        // wait for lock
        while sysctrl.pclksr.read().dflllckc().bit_is_clear() || sysctrl.pclksr.read().dflllckf().bit_is_clear() {}
    }

    // Feed DFLL48 into the main clock
    set_gclk_divider_and_source(gclk, GCLK0, 1, DFLL48M, true);
    // We are now running at 48Mhz

    // Disable 8MHz oscillator
    sysctrl.osc8m.write(|w| w.enable().clear_bit());

    pm.cpusel.write(|w| w.cpudiv().div1());
    pm.apbasel.write(|w| w.apbadiv().div1());
    pm.apbbsel.write(|w| w.apbbdiv().div1());
    pm.apbcsel.write(|w| w.apbcdiv().div1());
}
