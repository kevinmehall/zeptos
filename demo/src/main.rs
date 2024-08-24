#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use panic_probe as _;
use defmt_rtt as _;

use zeptos::Runtime;
use atsamd21e::CorePeripherals;
use cortex_m::peripheral::SYST;

const SYST_CSR_ENABLE: u32 = 1 << 0;
const SYST_CSR_TICKINT: u32 = 1 << 1;
const SYST_CSR_CLKSOURCE: u32 = 1 << 2;
const SYST_CSR_COUNTFLAG: u32 = 1 << 16;

#[zeptos::main]
async fn main(sp: Runtime) {
    let core = unsafe { CorePeripherals::steal() };
    let interrupt = zeptos::exception!(SysTick);

    defmt::info!("main");

    blink(sp).spawn(core.SYST, interrupt);
    task2(sp).spawn();
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