#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use panic_probe as _;

use zeptos::{ Runtime, Hardware, samd::gpio::{self, TypePin} };

#[zeptos::main]
async fn main(rt: Runtime, _hw: Hardware) {
    gpio::PB30::dirset();

    loop {
        rt.delay_us(100_000).await;
        gpio::PB30::outset();
        rt.delay_us(200_000).await;
        gpio::PB30::outclr();
    }
}
