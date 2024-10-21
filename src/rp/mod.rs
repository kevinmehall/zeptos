use rp_pac::{clocks::vals::{ClkAdcCtrlAuxsrc, ClkPeriCtrlAuxsrc, ClkRefCtrlSrc, ClkSysCtrlAuxsrc, ClkSysCtrlSrc, ClkUsbCtrlAuxsrc}, pll, resets::regs::Peripherals};
pub use rp_pac as pac;

mod rp_reg;
pub use rp_reg::RpReg;

pub mod gpio;

//pub mod clock;
//pub mod calibration;

#[cfg(feature="usb")]
pub mod usb;

//mod serial_number;
//pub use serial_number::serial_number;

const XOSC_HZ: u32 = 12_000_000;
const XOSC_STARTUP_DELAY_MS: u32 = 1;

const PLL_SYS_HZ: u32 = 125_000_000;
const PLL_USB_HZ: u32 = 48_000_000;

pub const CLK_REF_HZ: u32 = XOSC_HZ;
pub const CLK_SYS_HZ: u32 = PLL_SYS_HZ;
pub const CLK_PERI_HZ: u32 = PLL_USB_HZ;

pub(crate) fn init() {
    #![allow(unused_variables, unused_mut)]

    // Set clock to ROSC in case we're running from PLL before resetting it
    pac::CLOCKS.clk_sys_resus_ctrl().write_value(pac::clocks::regs::ClkSysResusCtrl(0));
    pac::CLOCKS.clk_sys_ctrl().modify(|w| w.set_src(ClkSysCtrlSrc::CLK_REF));
    while pac::CLOCKS.clk_sys_selected().read() != 1 << ClkSysCtrlSrc::CLK_REF as u32 {}
    pac::CLOCKS.clk_ref_ctrl().modify(|w| w.set_src(ClkRefCtrlSrc::ROSC_CLKSRC_PH));
    while pac::CLOCKS.clk_ref_selected().read() != 1 << ClkRefCtrlSrc::ROSC_CLKSRC_PH as u32 {}

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
    pac::CLOCKS.clk_ref_ctrl().modify(|w| w.set_src(ClkRefCtrlSrc::XOSC_CLKSRC));
    while pac::CLOCKS.clk_ref_selected().read() != 1 << ClkRefCtrlSrc::XOSC_CLKSRC as u32 {}

    // Enable PLLs
    configure_pll(pac::PLL_SYS, const { PllConfig::validate(1, 125, 6, 2, PLL_SYS_HZ)});
    configure_pll(pac::PLL_USB, const { PllConfig::validate(1, 100, 5, 5, PLL_USB_HZ)});

    // Switch clk_sys to pll_sys
    pac::CLOCKS.clk_sys_ctrl().write(|w| {
        w.set_auxsrc(ClkSysCtrlAuxsrc::CLKSRC_PLL_SYS);
        w.set_src(ClkSysCtrlSrc::CLKSRC_CLK_SYS_AUX);
    });
    while pac::CLOCKS.clk_sys_selected().read() != 1 << ClkSysCtrlSrc::CLKSRC_CLK_SYS_AUX as u32 {}

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

    #[cfg(feature = "usb")]
    enable.set_usbctrl(true);

    pac::RESETS.reset().write_value_clear(enable);
    while ((!pac::RESETS.reset_done().read().0) & enable.0) != 0 {}
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

#[cfg(feature = "rp2040-boot2-w25q080")]
#[link_section = ".boot2"]
#[used]
static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;