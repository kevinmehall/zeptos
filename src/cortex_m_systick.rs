use core::cell::SyncUnsafeCell;

use cortex_m::peripheral::SYST;
use cortex_m_rt::exception;

use crate::{time::{tick, Instant}, Runtime};

const SYST_CSR_ENABLE: u32 = 1 << 0;
const SYST_CSR_TICKINT: u32 = 1 << 1;
const SYST_CSR_CLKSOURCE: u32 = 1 << 2;

pub(crate) fn init() {
    unsafe {
        let syst = &*SYST::PTR;
        syst.rvr.write(crate::CLOCK_HZ / 1_000);
        syst.cvr.write(0);
        syst.csr.write(SYST_CSR_ENABLE | SYST_CSR_CLKSOURCE | SYST_CSR_TICKINT);
    }
}

static NOW: SyncUnsafeCell<u32> = SyncUnsafeCell::new(0);

#[exception]
fn SysTick() {
    unsafe {
        let now = &mut *(NOW.get());
        *now = (*now).wrapping_add(1000);
    };
    unsafe { tick(Runtime::steal(), now()) };
}

pub(crate) fn now() -> Instant{
    unsafe { Instant(*(NOW.get())) }
}

pub(crate) fn schedule(_time: Option<Instant>) {
    // no-op, we're going to call `tick()` every millisecond anyway
}

