#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use panic_probe as _;
use defmt_rtt as _;

use zeptos::{ Runtime, Hardware, rp::gpio::{self, TypePin, Function} };

#[zeptos::main]
async fn main(rt: Runtime, _hw: Hardware) {
    defmt::info!("init");
    gpio::GPIO25::set_function(Function::F5);
    gpio::GPIO25::oe_set();

    task1(rt).spawn(rt);

    loop {
        rt.delay_us(100_000).await;
        gpio::GPIO25::out_set();
        rt.delay_us(100_000).await;
        gpio::GPIO25::out_clr();
    }
}

#[zeptos::task]
async fn task1(rt: Runtime) {
    loop {
        rt.delay_us(1_000_000).await;
        defmt::info!("task1 loop {=u32}", rt.now().0);
    }
}
