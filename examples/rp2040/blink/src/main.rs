#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use panic_probe as _;
use defmt_rtt;

use zeptos::{ Runtime, Hardware, rp::gpio::{self, TypePin, Function} };

#[zeptos::main]
async fn main(_sp: Runtime, hw: Hardware) {
    defmt::info!("init");
    gpio::GPIO25::set_function(Function::F5);
    gpio::GPIO25::oe_set();
    let mut syst = hw.syst;

    loop {
        syst.delay(12_000_000).await;
        gpio::GPIO25::out_set();
        syst.delay(12_000_000).await;
        gpio::GPIO25::out_clr();
        defmt::info!("blink");
    }
}
