use core::mem;
use core::ops::Deref;
use core::sync::atomic::{compiler_fence, AtomicPtr, AtomicU16, AtomicU32, AtomicU8, Ordering};
use core::task::{Context, Poll};

use crate::executor::{Interrupt, TaskOnly};
use crate::samd::calibration;
use crate::samd::pac::{
    self, interrupt,
    usb::{
        device::{EPCFG, EPINTENCLR, EPINTENSET, EPINTFLAG, EPSTATUS, EPSTATUSCLR, EPSTATUSSET},
        DEVICE,
    },
    USB,
};
use crate::usb::{Event, UsbBuffer};
use crate::Runtime;
use defmt::{assert, debug_assert};
use usb::endpoint_address::{DIR_MASK as EP_DIR_MASK, IN as EP_IN, OUT as EP_OUT};

use super::gpio::{Alternate, TypePin};

pub unsafe fn usb_regs() -> &'static DEVICE {
    #[cfg(feature = "samd21")]
    unsafe {
        &(*USB::ptr()).device()
    }

    #[cfg(feature = "samd11")]
    unsafe {
        &(*USB::ptr()).device
    }
}

#[inline(always)]
pub fn ep_regs(regs: &DEVICE, ep: u8) -> &DEVICE_EP {
    assert!(ep < 8);
    unsafe {
        &*(regs as *const DEVICE)
            .cast::<DEVICE_EP>()
            .byte_offset(0x100 + 0x20 * (ep as isize))
    }
}

#[allow(non_camel_case_types)]
pub struct DEVICE_EP {
    #[doc = "+0x00 - DEVICE End Point Configuration"]
    pub epcfg: EPCFG,
    _reserved1: [u8; 0x03],
    #[doc = "+0x04 - DEVICE End Point Pipe Status Clear"]
    pub epstatusclr: EPSTATUSCLR,
    #[doc = "+0x05 - DEVICE End Point Pipe Status Set"]
    pub epstatusset: EPSTATUSSET,
    #[doc = "+0x06 - DEVICE End Point Pipe Status"]
    pub epstatus: EPSTATUS,
    #[doc = "+0x07 - DEVICE End Point Interrupt Flag"]
    pub epintflag: EPINTFLAG,
    #[doc = "+0x08 - DEVICE End Point Interrupt Clear Flag"]
    pub epintenclr: EPINTENCLR,
    #[doc = "+0x09 - DEVICE End Point Interrupt Set Flag"]
    pub epintenset: EPINTENSET,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PacketSize {
    Size8 = 0,
    Size16 = 1,
    Size32 = 2,
    Size64 = 3,
    Size128 = 4,
    Size256 = 5,
    Size512 = 6,
    Size1023 = 7,
}

impl PacketSize {
    pub const fn value(self) -> usize {
        match self {
            PacketSize::Size8 => 8,
            PacketSize::Size16 => 16,
            PacketSize::Size32 => 32,
            PacketSize::Size64 => 64,
            PacketSize::Size128 => 128,
            PacketSize::Size256 => 256,
            PacketSize::Size512 => 512,
            PacketSize::Size1023 => 1023,
        }
    }
}

/// Per-endpoint data descriptor accessed by the hardware but stored in RAM.
///
/// We can model the DMA access as if it were another thread, so use
/// atomic types, but all synchronization is done via registers,
/// so can be relaxed.
#[repr(C, align(4))]
pub struct EndpointBank {
    addr: AtomicPtr<u8>,
    pcksize: AtomicU32,
    extreg: AtomicU16,
    status_bk: AtomicU8,
    _reserved: [u8; 5],
}

impl EndpointBank {
    pub fn prepare_out(&self, packet_size: PacketSize, ptr: *mut u8, len: usize) {
        debug_assert!(len % (1 << (packet_size as u8 + 3)) == 0);
        debug_assert!(len < (1 << 14));
        self.addr.store(ptr, Ordering::Relaxed);
        self.pcksize.store(
            (len as u32) << 14 // MULTI_PACKET_SIZE
            | (packet_size as u8 as u32) << 28, // SIZE
            Ordering::Relaxed,
        );
    }

    pub fn out_len(&self) -> usize {
        let pcksize = self.pcksize.load(Ordering::Relaxed);
        (pcksize & ((1 << 14) - 1)) as usize
    }

    pub fn prepare_in(&self, packet_size: PacketSize, ptr: *mut u8, len: usize) {
        debug_assert!(len < (1 << 14));
        self.addr.store(ptr, Ordering::Relaxed);
        self.pcksize.store(
            (len as u32) // BYTE_COUNT
            | (packet_size as u8 as u32) << 28, // SIZE
            Ordering::Relaxed,
        );
    }
}

static EP_RAM: [[EndpointBank; 2]; 8] = unsafe { mem::zeroed() };

//static mut SETUP_PACKET: UsbBuffer<10> = UsbBuffer::new();
pub(crate) static mut CONTROL_BUF: UsbBuffer<64> = UsbBuffer::new();

#[derive(Copy, Clone)]
pub struct UsbShared {
    rt: Runtime,
}

pub struct Usb {
    usb: UsbShared,
}

impl Deref for Usb {
    type Target = UsbShared;

    fn deref(&self) -> &Self::Target {
        &self.usb
    }
}

impl Usb {
    pub unsafe fn new(rt: Runtime) -> Self {
        Usb {
            usb: UsbShared { rt },
        }
    }

    pub fn shared(&self) -> UsbShared {
        self.usb
    }

    pub fn rt(&self) -> Runtime {
        self.usb.rt
    }

    pub fn enable(&mut self) {
        let usb = self.usb();

        usb.ctrla.write(|w| w.swrst().set_bit());
        while usb.syncbusy.read().swrst().bit_is_set() {}

        usb.padcal.write(|w| {
            w.transn().variant(calibration::usb_transn_cal());
            w.transp().variant(calibration::usb_transp_cal());
            w.trim().variant(calibration::usb_trim_cal())
        });

        usb.descadd
            .write(|w| unsafe { w.descadd().bits(EP_RAM.as_ptr() as u32) });

        usb.ctrla.write(|w| {
            w.mode().device();
            w.runstdby().set_bit();
            w.enable().set_bit()
        });

        while usb.syncbusy.read().enable().bit_is_set() {}

        unsafe {
            cortex_m::peripheral::NVIC::unmask(pac::Interrupt::USB);
        }
    }

    pub fn attach(&mut self) {
        crate::samd::gpio::PA25::set_alternate(Alternate::G);
        crate::samd::gpio::PA24::set_alternate(Alternate::G);

        self.usb().ctrlb.write(|w| {
            w.spdconf().fs();
            w.detach().clear_bit()
        });

        self.usb().intenset.write(|w| w.eorst().set_bit());
    }

    pub fn detach(&mut self) {
        self.usb().ctrlb.write(|w| w.detach().set_bit());

        self.usb().intenclr.write(|w| w.eorst().set_bit());

        self.ep(0).epintenclr.write(|w| w.rxstp().set_bit());
    }

    pub fn poll_event(&self, cx: &mut Context) -> Poll<Event> {
        NOTIFY_BUS_EVENT.get(self.rt).subscribe(cx.waker());

        let flags = self.usb().intflag.read();
        let ep_reg = self.ep(0);

        if flags.eorst().bit_is_set() {
            self.usb().intflag.write(|w| w.eorst().set_bit());
            Poll::Ready(Event::Reset)
        } else if ep_reg.epintflag.read().rxstp().bit_is_set() {
            // Reading rxstp true means we have access to the setup buffer
            compiler_fence(Ordering::Acquire);

            let setup = unsafe { CONTROL_BUF[..8].try_into().unwrap() };

            // once rxstp is cleared, the hardware may receive another packet
            compiler_fence(Ordering::Release);

            ep_reg.epintflag.write(|w| w.rxstp().set_bit());

            Poll::Ready(Event::Setup(setup))
        } else {
            Poll::Pending
        }
    }

    pub unsafe fn ep0(&self) -> Endpoint0 {
        Endpoint0 { usb: self.shared() }
    }
}

impl UsbShared {
    fn usb(&self) -> &DEVICE {
        unsafe { usb_regs() }
    }

    pub fn rt(&self) -> Runtime {
        self.rt
    }

    fn ep(&self, ep: u8) -> &DEVICE_EP {
        ep_regs(self.usb(), ep & 0b111)
    }

    fn ep_ram(&self, ep: u8) -> &EndpointBank {
        &EP_RAM[(ep & 0b111) as usize][(ep >> 7) as usize]
    }

    pub fn configure_ep0(&self) {
        let ptr = &raw mut CONTROL_BUF as *mut u8;
        self.ep_ram(0).prepare_out(PacketSize::Size64, ptr, 64);
        let ep_reg = self.ep(0);
        
        ep_reg.epcfg.write(|w| {
            w.eptype0().variant(1);
            w.eptype1().variant(1)
        });

        ep_reg.epintenset.write(|w| w.rxstp().set_bit());
    }

    pub fn stall_ep0(&self) {
        self.ep(0).epstatusset.write(|w| {
            w.stallrq0().set_bit();
            w.stallrq1().set_bit()
        })
    }

    pub fn set_address(&self, addr: u8) {
        self.usb().dadd.write(|w| {
            w.adden().set_bit();
            w.dadd().variant(addr)
        });
    }

    pub fn enable_ep(&self, ep: u8) {
        if ep & EP_DIR_MASK == EP_IN {
            // IN
            self.ep(ep).epcfg.modify(|_, w| w.eptype1().variant(3));
        } else {
            // OUT
            self.ep(ep).epcfg.modify(|_, w| w.eptype0().variant(3));
        }
    }

    pub fn disable_ep(&self, ep: u8) {
        if ep & EP_DIR_MASK == EP_IN {
            // IN
            self.ep(ep).epcfg.modify(|_, w| w.eptype1().variant(0));
        } else {
            // OUT
            self.ep(ep).epcfg.modify(|_, w| w.eptype0().variant(0));
        }
    }

    pub async unsafe fn transfer_in(&self, ep: u8, ptr: *const u8, mut len: usize, zlp: bool) {
        assert!(ep & EP_DIR_MASK == EP_IN);

        let ep_reg = self.ep(ep);
        let ep_ram = self.ep_ram(ep);

        ep_reg.epintflag.write(|w| {
            w.trcpt1().set_bit();
            w.trfail1().set_bit()
        });

        scopeguard::defer! {
            ep_reg.epstatusclr.write(|w| {
                w.bk1rdy().set_bit()
            });
            ep_reg.epintenclr.write(|w| {
                w.trcpt1().set_bit()
            });
        }

        loop {
            ep_ram.prepare_in(PacketSize::Size64, ptr.cast_mut(), len);

            // Writing to start the transfer gives hardware control of the buffer
            compiler_fence(Ordering::SeqCst);

            ep_reg.epstatusset.write(|w| w.bk1rdy().set_bit());
            ep_reg.epintenset.write(|w| w.trcpt1().set_bit());

            NOTIFY_EP_IN.get(self.rt)[(ep & 0b111) as usize]
                .until(|| ep_reg.epintflag.read().trcpt1().bit_is_set())
                .await;

            // Reading trcpt1 means the hardware is done reading the buffer
            compiler_fence(Ordering::SeqCst);

            if zlp && len > 0 && len % 64 == 0 {
                // Send a zero-length packet. The PCKSIZE.AUTO_ZLP bit could do this in hardware,
                // but might be buggy -- observed AUTO_ZLP on one endpoint causing zero-length
                // packets to be sent on a different endpoint that did not set AUTO_ZLP.
                len = 0;
                continue;
            } else {
                break;
            }
        }
    }

    pub async unsafe fn transfer_out(&self, ep: u8, ptr: *mut u8, len: usize) -> usize {
        assert!(ep & EP_DIR_MASK == EP_OUT);

        let ep_reg = self.ep(ep);
        let ep_ram = self.ep_ram(ep);

        ep_ram.prepare_out(PacketSize::Size64, ptr, len);

        // Writing to start the transfer gives hardware control of the buffer
        compiler_fence(Ordering::Release);

        ep_reg.epintflag.write(|w| {
            w.trcpt0().set_bit();
            w.trfail0().set_bit()
        });
        ep_reg.epstatusclr.write(|w| w.bk0rdy().set_bit());
        ep_reg.epintenset.write(|w| w.trcpt0().set_bit());

        scopeguard::defer! {
            ep_reg.epstatusset.write(|w| {
                w.bk0rdy().set_bit()
            });
            ep_reg.epintenclr.write(|w| {
                w.trcpt0().set_bit()
            });
        }

        NOTIFY_EP_OUT.get(self.rt)[(ep & 0b111) as usize]
            .until(|| ep_reg.epintflag.read().trcpt0().bit_is_set())
            .await;

        // Reading trcpt0 means the hardware is done reading the buffer
        compiler_fence(Ordering::Acquire);

        ep_ram.out_len()
    }
}

pub struct Endpoint0 {
    pub(crate) usb: UsbShared
}

impl Endpoint0 {
    pub async fn ep0_transfer_in(&mut self, mut data: &[u8], is_full: bool) {
        loop {
            // We want to be able to send an arbitrary slice, which may not be
            // correctly aligned or in RAM (USB DMA can't read from flash), so copy
            // a packet at a time to CONTROL_BUF
            let buf = unsafe { &mut * &raw mut CONTROL_BUF };

            let (pkt, remaining) = data.split_at(data.len().min(buf.len()));

            buf[..pkt.len()].copy_from_slice(pkt);

            unsafe {
                self.usb.transfer_in(0x80, buf.as_ptr(), pkt.len(), false)
                    .await;
            }

            if pkt.len() < 64 || (remaining.len() == 0 && is_full) {
                break;
            }

            data = remaining;
        }
    }
    
    pub async fn ep0_transfer_out(&mut self) -> &[u8] {
        let buf = unsafe { &mut * &raw mut CONTROL_BUF };

        unsafe {
            self.usb.transfer_out(0, buf.as_mut_ptr(), 64).await;
        }

        &**buf
    }
    
    pub(crate) fn stall_ep0(&mut self) {
        self.usb.stall_ep0();
    }
}

static NOTIFY_BUS_EVENT: TaskOnly<Interrupt> = unsafe { TaskOnly::new(Interrupt::new()) };

static NOTIFY_EP_IN: TaskOnly<[Interrupt; 8]> =
    unsafe { TaskOnly::new([const { Interrupt::new() }; 8]) };
static NOTIFY_EP_OUT: TaskOnly<[Interrupt; 8]> =
    unsafe { TaskOnly::new([const { Interrupt::new() }; 8]) };

#[interrupt]
fn USB() {
    let usb = unsafe { usb_regs() };

    let flags = usb.intflag.read();
    if flags.eorst().bit_is_set() || ep_regs(usb, 0).epintflag.read().rxstp().bit_is_set() {
        unsafe { NOTIFY_BUS_EVENT.get_unchecked().notify() };
    }

    let summary = usb.epintsmry.read().bits();

    for ep in 0..8 {
        let mask = 1 << ep;
        if summary & mask != 0 {
            let regs = ep_regs(usb, ep);
            let flags = regs.epintflag.read();

            if flags.trcpt0().bit() {
                unsafe { NOTIFY_EP_OUT.get_unchecked()[ep as usize].notify() };
            }

            if flags.trcpt1().bit() {
                unsafe { NOTIFY_EP_IN.get_unchecked()[ep as usize].notify() };
            }
        }
    }
}
