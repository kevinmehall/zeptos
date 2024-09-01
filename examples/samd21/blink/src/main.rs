#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use panic_probe as _;

use zeptos::{ Runtime, Hardware, samd::gpio::{self, TypePin} };

#[zeptos::main]
async fn main(_sp: Runtime, hw: Hardware) {
    gpio::PB30::dirset();
    let mut syst = hw.syst;

    loop {
        syst.delay_us(100_000).await;
        gpio::PB30::outset();
        syst.delay_us(200_000).await;
        gpio::PB30::outclr();
    }
}
