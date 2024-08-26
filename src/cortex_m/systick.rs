use cortex_m::peripheral::{syst, SYST};
use cortex_m_rt::exception;

use crate::{executor::{Interrupt, TaskOnly}, Runtime};

const SYST_CSR_ENABLE: u32 = 1 << 0;
const SYST_CSR_TICKINT: u32 = 1 << 1;
const SYST_CSR_CLKSOURCE: u32 = 1 << 2;
const SYST_CSR_COUNTFLAG: u32 = 1 << 16;

pub struct SysTick {
    runtime: Runtime,
}

static INT: TaskOnly<Interrupt> = unsafe { TaskOnly::new(Interrupt::new()) };

#[exception]
fn SysTick() {
    unsafe { INT.get_unchecked().notify() }
}

impl SysTick {
    pub(crate) unsafe fn init(runtime: Runtime) -> SysTick {
        SysTick { runtime }
    }

    pub fn registers(&self) -> &syst::RegisterBlock {
        unsafe { &*SYST::PTR }
    }

    pub async fn delay(&mut self, ticks: u32) {
        unsafe {
            self.registers().rvr.write(ticks);
            self.registers().cvr.write(0);
            self.registers().csr.write(SYST_CSR_ENABLE | SYST_CSR_CLKSOURCE | SYST_CSR_TICKINT);
        }

        INT.get(self.runtime).until(|| {
            self.registers().csr.read() & SYST_CSR_COUNTFLAG != 0
        }).await;
        unsafe { self.registers().csr.write(0); }
    }
}

