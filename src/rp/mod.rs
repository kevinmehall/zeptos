//! Hardware support for RP2040 and RP2350.
#[allow(unused_imports)]
use rp_pac::{clocks::vals::{ClkAdcCtrlAuxsrc, ClkPeriCtrlAuxsrc, ClkRefCtrlSrc, ClkSysCtrlAuxsrc, ClkSysCtrlSrc, ClkUsbCtrlAuxsrc}, pll, resets::regs::Peripherals, Interrupt};
pub use rp_pac as pac;

mod rp_reg;
pub use rp_reg::RpReg;

pub mod gpio;

#[cfg(feature = "rp2040")]
pub mod rom_data;

#[cfg(feature = "rp2040")]
pub mod flash;

#[cfg(feature="time")]
pub(crate) mod timer;

#[cfg(feature="usb")]
pub(crate) mod usb;

pub mod i2c;
pub mod spi;

#[cfg(all(feature = "rp2040", feature = "rp2040-boot2-w25q080"))]
#[unsafe(link_section = ".boot2")]
#[used]
static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;

#[cfg(feature = "rp2350")]
#[unsafe(link_section = ".boot_info")]
#[used]
static BOOT_BLOCK: [u32; 5] = [
    0xffffded3, // PICOBIN_BLOCK_MARKER_START
    0x10210142, // Executable, secure mode, Arm, RP2350
    0x000001ff, // Last
    0x00000000, // Relative pointer to close block loop
    0xab123579, // PICOBIN_BLOCK_MARKER_END
];

const XOSC_HZ: u32 = 12_000_000;
const XOSC_STARTUP_DELAY_MS: u32 = 1;

const PLL_SYS_HZ: u32 = super::CLOCK_HZ;
const PLL_USB_HZ: u32 = 48_000_000;

pub const CLK_REF_HZ: u32 = XOSC_HZ;
pub const CLK_SYS_HZ: u32 = PLL_SYS_HZ;
pub const CLK_PERI_HZ: u32 = PLL_USB_HZ;

pub(crate) fn init() {
    #![allow(unused_variables, unused_mut)]

    cfg_select! {
        feature="rp2040" => {
            fn set_clk_sys_src(src: ClkSysCtrlSrc) {
                pac::CLOCKS.clk_sys_ctrl().modify(|w| w.set_src(src));
                while pac::CLOCKS.clk_sys_selected().read() != 1 << src as u32 {}
            }

            fn set_clk_ref_src(src: ClkRefCtrlSrc) {
                pac::CLOCKS.clk_ref_ctrl().modify(|w| w.set_src(src));
                while pac::CLOCKS.clk_ref_selected().read() != 1 << src as u32 {}
            }
        }

        feature="rp2350" => {
            // rp-pac has a dedicated type for these registers on rp2350
            fn set_clk_sys_src(src: ClkSysCtrlSrc) {
                pac::CLOCKS.clk_sys_ctrl().modify(|w| w.set_src(src));
                while pac::CLOCKS.clk_sys_selected().read().0 != 1 << src as u32 {}
            }

            fn set_clk_ref_src(src: ClkRefCtrlSrc) {
                pac::CLOCKS.clk_ref_ctrl().modify(|w| w.set_src(src));
                while pac::CLOCKS.clk_ref_selected().read().0 != 1 << src as u32 {}
            }
        }
    }

    // Set clock to ROSC in case we're running from PLL before resetting it
    pac::CLOCKS.clk_sys_resus_ctrl().write_value(pac::clocks::regs::ClkSysResusCtrl(0));
    set_clk_sys_src(ClkSysCtrlSrc::CLK_REF);
    set_clk_ref_src(ClkRefCtrlSrc::ROSC_CLKSRC_PH);

    // Reset all peripherals (except those we need to keep executing code)
    let mut to_reset = Peripherals(0x01ff_ffff);
    to_reset.set_io_qspi(false);
    to_reset.set_pads_qspi(false);
    pac::RESETS.reset().write_value_set(to_reset);

    // Take PLLs out of reset
    let mut enable = Peripherals::default();
    enable.set_pll_sys(true);
    enable.set_pll_usb(true);
    pac::RESETS.reset().write_value_clear(enable);
    while ((!pac::RESETS.reset_done().read().0) & enable.0) != 0 {}

    // Start XOSC
    let startup_delay = (((XOSC_HZ / 1000) * XOSC_STARTUP_DELAY_MS) + 128) / 256;
    pac::XOSC.startup().write(|w| w.set_delay(startup_delay as u16));
    pac::XOSC.ctrl().write(|w| {
        w.set_freq_range(pac::xosc::vals::CtrlFreqRange::_1_15MHZ);
        w.set_enable(pac::xosc::vals::Enable::ENABLE);
    });
    while !pac::XOSC.status().read().stable() {}

    // Switch clk_ref to XOSC
    set_clk_ref_src(ClkRefCtrlSrc::XOSC_CLKSRC);

    // Enable PLLs
    configure_pll(pac::PLL_SYS, cfg_select! {
        feature = "rp2040" => const { PllConfig::validate(1, 125, 6, 2, PLL_SYS_HZ) },
        feature = "rp2350" => const { PllConfig::validate(1, 125, 5, 2, PLL_SYS_HZ) },
    });
    configure_pll(pac::PLL_USB, const { PllConfig::validate(1, 100, 5, 5, PLL_USB_HZ)});

    // Switch clk_sys to pll_sys
    pac::CLOCKS.clk_sys_ctrl().modify(|w| w.set_auxsrc(ClkSysCtrlAuxsrc::CLKSRC_PLL_SYS));
    set_clk_sys_src(ClkSysCtrlSrc::CLKSRC_CLK_SYS_AUX);

    // Enable clk_peri
    pac::CLOCKS.clk_peri_ctrl().write(|w| {
        w.set_enable(true);
        w.set_auxsrc(ClkPeriCtrlAuxsrc::CLKSRC_PLL_USB);
    });

    // Enable clk_usb
    #[cfg(feature = "usb")]
    pac::CLOCKS.clk_usb_ctrl().write(|w| {
        w.set_enable(true);
        w.set_auxsrc(ClkUsbCtrlAuxsrc::CLKSRC_PLL_USB);
    });

    // Enable_clk_adc
    pac::CLOCKS.clk_adc_ctrl().write(|w| {
        w.set_enable(true);
        w.set_auxsrc(ClkAdcCtrlAuxsrc::CLKSRC_PLL_USB);
    });

    // Take peripherals out of reset
    let mut enable = Peripherals::default();
    enable.set_io_bank0(true);
    enable.set_io_qspi(true);
    enable.set_pads_bank0(true);
    enable.set_pads_qspi(true);
    enable.set_syscfg(true);
    enable.set_sysinfo(true);
    enable.set_busctrl(true);
    #[cfg(feature = "usb")] enable.set_usbctrl(true);
    #[cfg(all(feature = "time", feature="rp2040"))] enable.set_timer(true);
    #[cfg(all(feature = "time", feature="rp2350"))] enable.set_timer0(true);

    pac::RESETS.reset().write_value_clear(enable);
    while ((!pac::RESETS.reset_done().read().0) & enable.0) != 0 {}

    #[cfg(feature = "rp2040")]
    unsafe {
        // SAFETY: interrupts are disabled on init and core 1 is halted
        flash::flash_unique_id(&mut * &raw mut serial_number::FLASH_UID, true);
    }

    #[allow(unused_unsafe)]
    unsafe {
        #[cfg(feature = "gpio-interrupts")]
        cortex_m::peripheral::NVIC::unmask(Interrupt::IO_IRQ_BANK0);
        #[cfg(feature = "i2c0")]
        cortex_m::peripheral::NVIC::unmask(Interrupt::I2C0_IRQ);
        #[cfg(feature = "i2c1")]
        cortex_m::peripheral::NVIC::unmask(Interrupt::I2C1_IRQ);
        #[cfg(feature = "spi0")]
        cortex_m::peripheral::NVIC::unmask(Interrupt::SPI0_IRQ);
        #[cfg(feature = "spi1")]
        cortex_m::peripheral::NVIC::unmask(Interrupt::SPI1_IRQ);
        #[cfg(all(feature = "time", feature = "rp2040"))]
        cortex_m::peripheral::NVIC::unmask(Interrupt::TIMER_IRQ_0);
        #[cfg(all(feature = "time", feature = "rp2350"))]
        cortex_m::peripheral::NVIC::unmask(Interrupt::TIMER0_IRQ_0);
    }
}

struct PllConfig {
    refdiv: u8,
    fbdiv: u16,
    post_div_1: u8,
    post_div_2: u8
}

impl PllConfig {
    const fn validate(refdiv: u8, fbdiv: u16, post_div_1: u8, post_div_2: u8, out_hz: u32) -> PllConfig {
        let ref_freq = XOSC_HZ / refdiv as u32;
        assert!(fbdiv >= 16 && fbdiv <= 320);
        assert!(post_div_1 >= 1 && post_div_1 <= 7);
        assert!(post_div_2 >= 1 && post_div_2 <= 7);
        assert!(refdiv >= 1 && refdiv <= 63);
        assert!(ref_freq >= 5_000_000 && ref_freq <= 800_000_000);
        let vco_freq = ref_freq.saturating_mul(fbdiv as u32);
        assert!(vco_freq >= 750_000_000 && vco_freq <= 1_800_000_000);
        assert!(vco_freq / post_div_1 as u32 / post_div_2 as u32 == out_hz);
        PllConfig { refdiv, fbdiv, post_div_1, post_div_2 }
    }
}

fn configure_pll(p: pac::pll::Pll, config: PllConfig) {
    // Load VCO-related dividers before starting VCO
    p.cs().write(|w| w.set_refdiv(config.refdiv));
    p.fbdiv_int().write(|w| w.set_fbdiv_int(config.fbdiv));

    // Turn on PLL
    let mut pwr = pll::regs::Pwr::default();
    pwr.set_dsmpd(true); // "nothing is achieved by setting this low"
    pwr.set_pd(false);
    pwr.set_vcopd(false);
    pwr.set_postdivpd(true);

    p.pwr().write_value(pwr);

    // Wait for PLL to lock
    while !p.cs().read().lock() {}

    // Set post-dividers
    p.prim().write(|w| {
        w.set_postdiv1(config.post_div_1);
        w.set_postdiv2(config.post_div_2);
    });

    // Turn on post divider
    pwr.set_postdivpd(false);
    p.pwr().write_value(pwr);
}


pub(crate) mod serial_number {
    /// Length of the array returned by `serial_number()`.
    pub const SERIAL_NUMBER_LEN: usize = 8;

    cfg_select! {
        feature = "rp2040" => {
            pub(crate) static mut FLASH_UID: [u8; 8] = [0; SERIAL_NUMBER_LEN];

            /// Get the unique ID of the flash device.
            pub fn serial_number() -> [u8; SERIAL_NUMBER_LEN] {
                // SAFETY: This is initialized at boot and not written thereafter
                unsafe { * &raw const FLASH_UID }
            }
        }
        feature = "rp2350" => {
            pub fn serial_number() -> [u8; SERIAL_NUMBER_LEN] {
                // Big-endian order to match boot ROM USB serial
                let otp = rp_pac::OTP_DATA;
                let data = [
                    otp.chipid3().read().to_be_bytes(),
                    otp.chipid2().read().to_be_bytes(),
                    otp.chipid1().read().to_be_bytes(),
                    otp.chipid0().read().to_be_bytes(),
                ];
                // Safety: Memory layout of [[u8; 2]; 4] and [u8; 8] guaranteed compatible
                unsafe { core::mem::transmute(data) }
            }
        }
    }
}
