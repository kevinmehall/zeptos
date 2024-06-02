#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use panic_probe as _;
use defmt_rtt as _;

use zeptos::Spawner;
use atsamd21e::{ interrupt, CorePeripherals };
use cortex_m_rt::exception;
use cortex_m::peripheral::{syst, SYST};

const SYST_CSR_ENABLE: u32 = 1 << 0;
const SYST_CSR_TICKINT: u32 = 1 << 1;
const SYST_CSR_CLKSOURCE: u32 = 1 << 2;
const SYST_CSR_COUNTFLAG: u32 = 1 << 16;


#[zeptos::main]
async fn main(sp: Spawner) {
    let mut core = unsafe { CorePeripherals::steal() };
    let interrupt = zeptos::exception!(SysTick);

    defmt::info!("main");

    sp.spawn_task(blink(core.SYST, interrupt));
    sp.spawn_task(task2());
}

#[zeptos::task]
async fn blink(syst: SYST, interrupt: &'static zeptos::Interrupt) {
    defmt::info!("task");

    unsafe {
        syst.rvr.write(1_000_000);
        syst.cvr.write(0);
        syst.csr.write(SYST_CSR_ENABLE | SYST_CSR_CLKSOURCE | SYST_CSR_TICKINT);
    }

    loop {
        interrupt.until(|| {
            syst.csr.read() & SYST_CSR_COUNTFLAG != 0
        }).await;
        unsafe { syst.csr.write(SYST_CSR_ENABLE | SYST_CSR_CLKSOURCE | SYST_CSR_TICKINT); }
        defmt::info!("tick");
    }
}

#[zeptos::task]
async fn task2() {
    defmt::info!("task2");
}