use crate::{Runtime, rp::RpReg, time::{Instant, tick}};
use crate ::rp::pac::interrupt;

cfg_select! {
    feature = "rp2040" => {
        use crate::rp::pac::{TIMER, WATCHDOG};
        const IRQ: rp_pac::Interrupt = rp_pac::Interrupt::TIMER_IRQ_0;

        #[interrupt]
        fn TIMER_IRQ_0() {
            isr();
        }
    }
    feature = "rp2350" => {
        use crate::rp::pac::{TIMER0 as TIMER, TICKS};
        const IRQ: rp_pac::Interrupt = rp_pac::Interrupt::TIMER0_IRQ_0;

        #[interrupt]
        fn TIMER0_IRQ_0() {
            isr();
        }
    }
}

pub(crate) fn init() {
    cfg_select! {
        feature = "rp2040" => {
            WATCHDOG.tick().write(|w| {
                w.set_cycles((super::CLK_REF_HZ / 1_000_000) as u16);
                w.set_enable(true);
            });
        }
        feature = "rp2350" => {
            TICKS.timer0_cycles().write(|w| w.set_timer0_cycles((super::CLK_REF_HZ / 1_000_000) as u16));
            TICKS.timer0_ctrl().write(|w| w.set_enable(true));
        }
    }

    TIMER.inte().write_set(|w| w.set_alarm(0, true));
}

pub(crate) fn now() -> Instant{
    Instant(TIMER.timerawl().read())
}

pub(crate) fn schedule(time: Option<Instant>) {
    if let Some(time) = time {
        TIMER.alarm(0).write_value(time.0);
        if now().is_after(time) {
            // If the time has already passed, pend the interrupt immediately.
            cortex_m::peripheral::NVIC::pend(IRQ);
        }
    }
}

#[inline(always)]
fn isr() {
    TIMER.intr().write_clear(|w| w.set_alarm(0, true));
    unsafe { tick(Runtime::steal(), now()) };
}
