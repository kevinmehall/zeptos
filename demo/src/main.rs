#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use panic_probe as _;
use defmt_rtt as _;

use zeptos::{ Runtime, Hardware };
use atsamd21e::CorePeripherals;

#[zeptos::main]
async fn main(_sp: Runtime, hw: Hardware) {
    defmt::info!("main");
    let mut syst = hw.syst;

    loop {
        syst.delay(1_000_000).await;
        defmt::info!("tick");
    }
}
