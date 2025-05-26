#[cfg(feature="samd11")]
pub use atsamd11d as pac;

#[cfg(feature="samd21")]
pub use atsamd21j as pac;

pub mod gpio;

pub mod clock;
pub mod calibration;

#[cfg(feature="usb")]
pub mod usb;

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

    #[cfg(feature="usb")]
    crate::samd::clock::enable_clock(&mut gclk, crate::samd::pac::gclk::clkctrl::IDSELECT_A::USB, crate::samd::pac::gclk::clkctrl::GENSELECT_A::GCLK0);
}

