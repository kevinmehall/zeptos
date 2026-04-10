//! Hardware support for SAM D11 and SAM D21 microcontrollers.

#[cfg(feature="samd11")]
pub use atsamd11d as pac;

#[cfg(feature="samd21")]
pub use atsamd21j as pac;

#[allow(unused_imports)]
use pac::gclk::clkctrl;

pub mod gpio;
pub mod sercom;

pub mod clock;
pub mod calibration;

#[cfg(feature="usb")]
pub(crate) mod usb;

pub(crate) mod serial_number;

pub(crate) fn init() {
    #![allow(unused_variables, unused_mut)]

    #[cfg(any(
        feature="samd-clock-48m-usb",
        feature="samd-clock-48m-internal",
        feature="samd-clock-48m-external-32k-osc",
        feature="samd-clock-48m-external-32k-xtal",
    ))]
    crate::samd::clock::configure_clocks();

    let pm = unsafe { crate::samd::pac::PM::steal() };
    let mut gclk = unsafe { crate::samd::pac::GCLK::steal() };

    pm.ahbmask.write(|w| {
        #[cfg(feature="usb")]
        w.usb_().set_bit();

        w
    });

    pm.apbcmask.write(|w| {
        #[cfg(feature="sercom0")] w.sercom0_().set_bit();
        #[cfg(feature="sercom1")] w.sercom1_().set_bit();
        #[cfg(feature="sercom2")] w.sercom2_().set_bit();
        #[cfg(feature="sercom3")] w.sercom3_().set_bit();
        #[cfg(feature="sercom4")] w.sercom4_().set_bit();
        #[cfg(feature="sercom5")] w.sercom5_().set_bit();
        w
    });

    #[cfg(feature="usb")]
    crate::samd::clock::enable_clock(&mut gclk, clkctrl::IDSELECT_A::USB, clkctrl::GENSELECT_A::GCLK0);

    #[cfg(feature="sercom0")]
    crate::samd::clock::enable_clock(&mut gclk, clkctrl::IDSELECT_A::SERCOM0_CORE, clkctrl::GENSELECT_A::GCLK0);

    #[cfg(feature="sercom1")]
    crate::samd::clock::enable_clock(&mut gclk, clkctrl::IDSELECT_A::SERCOM1_CORE, clkctrl::GENSELECT_A::GCLK0);

    #[cfg(feature="sercom2")]
    crate::samd::clock::enable_clock(&mut gclk, clkctrl::IDSELECT_A::SERCOM2_CORE, clkctrl::GENSELECT_A::GCLK0);

    #[cfg(feature="sercom3")]
    crate::samd::clock::enable_clock(&mut gclk, clkctrl::IDSELECT_A::SERCOM3_CORE, clkctrl::GENSELECT_A::GCLK0);

    #[cfg(feature="sercom4")]
    crate::samd::clock::enable_clock(&mut gclk, clkctrl::IDSELECT_A::SERCOM4_CORE, clkctrl::GENSELECT_A::GCLK0);

    #[cfg(feature="sercom5")]
    crate::samd::clock::enable_clock(&mut gclk, clkctrl::IDSELECT_A::SERCOM5_CORE, clkctrl::GENSELECT_A::GCLK0);

    #[allow(unused_unsafe)]
    unsafe {
        #[cfg(feature = "sercom0")]
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::SERCOM0);
        #[cfg(feature = "sercom1")]
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::SERCOM1);
        #[cfg(feature = "sercom2")]
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::SERCOM2);
        #[cfg(feature = "sercom3")]
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::SERCOM3);
        #[cfg(feature = "sercom4")]
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::SERCOM4);
        #[cfg(feature = "sercom5")]
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::SERCOM5);
    }

}
