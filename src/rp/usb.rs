use core::future::Future;
use core::task::{Context, Poll};
use core::{mem, slice};
use core::ops::Deref;
use core::sync::atomic::{compiler_fence, Ordering};

use crate::executor::{Interrupt, TaskOnly};
use crate::usb::Event;
use crate::Runtime;
use defmt::{assert, debug, debug_assert};
use rp_pac::common::{Reg, RW};
use rp_pac::usb::regs::{EpAbort, EpAbortDone};
use rp_pac::usb_dpram::regs::{EpBufferControl, EpControl};
use rp_pac::usb_dpram::vals::EpControlEndpointType;
use rp_pac::{interrupt, USB_DPRAM, USB};
use scopeguard::ScopeGuard;
use usb::endpoint_address::{DIR_MASK as EP_DIR_MASK, IN as EP_IN, OUT as EP_OUT, ADDR_MASK as EP_ADDR_MASK};

use super::{pac as pac, RpReg};

const EP_COUNT: usize = 16;
const EP_MEMORY_SIZE: usize = 4096;
const EP_MEMORY: *mut u8 = pac::USB_DPRAM.as_ptr() as *mut u8;

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

        // Clear DPRAM
        let dpram = EP_MEMORY as *mut u32;
        for i in 0..EP_MEMORY_SIZE / mem::size_of::<u32>() {
            unsafe { dpram.add(i).write_volatile(0) }
        }

        unsafe {
            cortex_m::peripheral::NVIC::unmask(pac::Interrupt::USBCTRL_IRQ);
        }

        usb.usb_muxing().write(|w| {
            w.set_to_phy(true);
            w.set_softcon(true);
        });

        usb.usb_pwr().write(|w| {
            w.set_vbus_detect(true);
            w.set_vbus_detect_override_en(true);
        });

        usb.main_ctrl().write(|w| {
            w.set_controller_en(true);
        });

        usb.sie_ctrl().write(|w| {
            w.set_ep0_int_1buf(true);
        });

        usb.inte().write(|w| {
            w.set_buff_status(true);
        });
    }

    pub fn attach(&mut self) {
        self.usb().sie_ctrl().write_set(|w| w.set_pullup_en(true));
        self.usb().inte().write_set(|w| {
            w.set_bus_reset(true);
            w.set_setup_req(true);
        });
    }

    pub fn detach(&mut self) {
        self.usb().sie_ctrl().write_clear(|w| w.set_pullup_en(true));
        self.usb().inte().write_clear(|w| {
            w.set_bus_reset(true);
            w.set_setup_req(true);
        });
    }

    pub fn poll_event(&self, cx: &mut Context<'_>) -> Poll<Event> {
        NOTIFY_BUS_EVENT
            .get(self.rt)
            .subscribe(cx.waker());


        let status = self.usb().sie_status().read();

        if status.bus_reset() {
            self.usb().buff_status().write(|w| w.0 = 0xFFFFFFFF);
            self.usb().addr_endp().write(|w| w.set_address(0));
            for i in 1..EP_COUNT {
                USB_DPRAM.ep_in_control(i - 1).write(|w| { w.set_enable(false) });
                USB_DPRAM.ep_out_control(i - 1).write(|w| { w.set_enable(false) });
            }
            self.usb().sie_status().write(|w| w.set_bus_reset(true));
            Poll::Ready(Event::Reset)
        } else if status.setup_rec() {
            // Reading setup_rec true means we have access to the setup buffer
            compiler_fence(Ordering::Acquire);

            let setup = unsafe { (EP_MEMORY as *const [u8; 8]).read() };

            compiler_fence(Ordering::Release);
            
            USB_DPRAM.ep_in_buffer_control(0).write(|w| w.set_pid(0, false));
            USB_DPRAM.ep_out_buffer_control(0).write(|w| w.set_pid(0, false));
            self.usb().ep_stall_arm().write(|w| {
                w.set_ep0_in(false);
                w.set_ep0_out(false);
            });
            self.usb().sie_status().write(|w| w.set_setup_rec(true));

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
    fn usb(&self) -> pac::usb::Usb {
        pac::USB
    }

    pub fn rt(&self) -> Runtime {
        self.rt
    }


    pub fn configure_ep0(&self) {
        
    }

    pub fn stall_ep0(&self) {
        USB_DPRAM.ep_in_buffer_control(0).write(|w| {
            w.set_stall(true);
        });
        USB_DPRAM.ep_out_buffer_control(0).write(|w| {
            w.set_stall(true);
        });
        self.usb().ep_stall_arm().write(|w| {
            w.set_ep0_in(true);
            w.set_ep0_out(true);
        });
    }

    pub fn set_address(&self, addr: u8) {
        self.usb().addr_endp().write(|w| {
            w.set_address(addr);
        });
    }


    fn ep_ctrl(&self, ep: u8) -> Reg<EpControl, RW> {
        assert!(ep & EP_ADDR_MASK != 0);
        if ep & EP_DIR_MASK == EP_IN {
            USB_DPRAM.ep_in_control((ep & EP_ADDR_MASK) as usize - 1)
        } else {
            USB_DPRAM.ep_out_control((ep & EP_ADDR_MASK) as usize - 1)
        }
    }

    fn ep_buffer_control(&self, ep: u8) -> Reg<EpBufferControl, RW> {
        if ep & EP_DIR_MASK == EP_IN {
            USB_DPRAM.ep_in_buffer_control((ep & EP_ADDR_MASK) as usize)
        } else {
            USB_DPRAM.ep_out_buffer_control((ep & EP_ADDR_MASK) as usize)
        }
    }

    /// Buffer address relative to start of DPRAM
    fn buffer_address(&self, ep: u8) -> u16 {
        if ep & 0xF == 0 {
            0x100
        } else {
            0x100 + 0x40 * ((ep & 0xF) << 1 | (ep >> 7)) as u16
        }
    }

    pub fn enable_ep(&self, ep: u8) {
        self.ep_buffer_control(ep).write(|w| {
            w.set_pid(0, true);
        });
        // if ep & EP_DIR_MASK == EP_IN {
        // } else {
        //     self.ep_buffer_control(ep).write(|w| {
        //         w.set_pid(0, false);
        //         w.set_length(0, 64);
        //     });
        // }

        self.ep_ctrl(ep).write(|w| {
            w.set_buffer_address(self.buffer_address(ep));
            w.set_double_buffered(false);
            w.set_endpoint_type(EpControlEndpointType::BULK);
            w.set_interrupt_per_buff(true);
            w.set_enable(true);
        });
    }

    pub fn disable_ep(&self, ep: u8) {
        self.ep_ctrl(ep).write(|w| { w.set_enable(false) });
    }

    fn cancel_on_drop(&self, ep: u8) -> ScopeGuard<(), impl FnOnce(()) + '_> {
        scopeguard::guard((), move |()| {
            debug!("canceling transfer on ep {:02x}", ep);

            let mask = 1u32 << (((ep >> 7) ^ 1) | (ep << 1));
            self.usb().ep_abort().write_value_set(EpAbort(mask));
            while self.usb().ep_abort_done().read().0 & mask == 0 {}

            compiler_fence(Ordering::SeqCst);

            let buffer_control = self.ep_buffer_control(ep);
            buffer_control.modify(|w| {
                if w.available(0) {
                    w.set_pid(0, !w.pid(0)); // undo toggle if not sent
                }
                w.set_available(0, false);
                w.set_full(0, false);
            });

            self.usb().ep_abort().write_value_clear(EpAbort(mask));
            self.usb().ep_abort_done().write_value_clear(EpAbortDone(mask));
        })
    }

    pub async unsafe fn transfer_in(&self, ep: u8, ptr: *const u8, len: usize, zlp: bool) {
        assert!(ep & EP_DIR_MASK == EP_IN);

        let slice = unsafe { slice::from_raw_parts(ptr, len) };
        let buf = unsafe { EP_MEMORY.add(self.buffer_address(ep) as usize) };
        let buffer_control = self.ep_buffer_control(ep);
        let mut pid = buffer_control.read().pid(0);

        debug_assert!(buffer_control.read().available(0) == false);

        let guard = self.cancel_on_drop(ep);

        for pkt in slice.chunks(64).chain((len == 0 || (zlp && len % 64 == 0)).then_some(&[] as &[u8]).into_iter()) {
            // Copy packet to DPRAM
            unsafe { buf.copy_from_nonoverlapping(pkt.as_ptr(), pkt.len()) }

            // Writing to start the transfer gives hardware control of the buffer
            compiler_fence(Ordering::Release);
            
            pid = !pid;
            buffer_control.write(|w| {
                w.set_pid(0, pid);
                w.set_length(0, pkt.len() as _);
                w.set_full(0, true);
            });
            cortex_m::asm::delay(12);
            buffer_control.write(|w| {
                w.set_pid(0, pid);
                w.set_length(0, pkt.len() as _);
                w.set_full(0, true);
                w.set_available(0, true);
            });
            
            debug!("start IN to {:02x}: {} bytes DATA{}", ep, pkt.len(), pid as u8);

            NOTIFY_EP_IN.get(self.rt)[(ep & 0xF) as usize]
                .until(|| !buffer_control.read().available(0))
                .await;

            // The hardware is done reading the buffer
            compiler_fence(Ordering::Acquire);

            debug!("completed IN to {:02x}: {} bytes", ep, pkt.len());
        }

        mem::forget(guard);
    }

    pub async unsafe fn transfer_out(&self, ep: u8, ptr: *mut u8, len: usize) -> usize {
        assert!(ep & EP_DIR_MASK == EP_OUT);

        let mut total_len = 0;
        let mut slice = unsafe { slice::from_raw_parts_mut(ptr, len) };
        let buf = unsafe { EP_MEMORY.add(self.buffer_address(ep) as usize) };
        let buffer_control = self.ep_buffer_control(ep);
        let mut pid = buffer_control.read().pid(0);

        debug_assert!(buffer_control.read().available(0) == false);

        let guard = self.cancel_on_drop(ep);

        loop {
            // Writing to start the transfer gives hardware control of the buffer
            compiler_fence(Ordering::Release);
            
            pid = !pid;
            buffer_control.write(|w| {
                w.set_pid(0, pid);
                w.set_length(0, 64);
            });
            cortex_m::asm::delay(12);
            buffer_control.write(|w| {
                w.set_pid(0, pid);
                w.set_length(0, 64);
                w.set_available(0, true);
            });
            
            debug!("start OUT to {:02x}: DATA{}", ep, pid as u8);

            let pkt_len = NOTIFY_EP_OUT.get(self.rt)[(ep & 0xF) as usize]
                .until(|| {
                    let c = buffer_control.read();
                    (!c.available(0)).then_some(c.length(0) as usize)
                })
                .await;

            // The hardware is done writing the buffer
            compiler_fence(Ordering::Acquire);

            debug!("completed OUT to {:02x}: {} bytes", ep, pkt_len);

            assert!(pkt_len <= slice.len());

            unsafe { slice.as_mut_ptr().copy_from_nonoverlapping(buf, pkt_len) }
            total_len += pkt_len;
            slice = &mut slice[pkt_len..];

            if pkt_len < 64 {
                break
            }
        }

        debug!("end transfer OUT to {:02x}: {} bytes", ep, total_len);

        mem::forget(guard);

        total_len
    }

    pub async unsafe fn out_packet(&self, ep: u8) -> (*mut u8, usize) {
        let buf = unsafe { EP_MEMORY.add(self.buffer_address(ep) as usize) };
        let buffer_control = self.ep_buffer_control(ep);
        let mut pid = buffer_control.read().pid(0);

        debug_assert!(buffer_control.read().available(0) == false);

        compiler_fence(Ordering::Release);
        
        pid = !pid;
        buffer_control.write(|w| {
            w.set_pid(0, pid);
            w.set_length(0, 64);
        });
        cortex_m::asm::delay(12);
        buffer_control.write(|w| {
            w.set_pid(0, pid);
            w.set_length(0, 64);
            w.set_available(0, true);
        });
        
        debug!("start OUT to {:02x}: DATA{}", ep, pid as u8);

        let len = NOTIFY_EP_OUT.get(self.rt)[(ep & 0xF) as usize]
            .until(|| {
                let c = buffer_control.read();
                (!c.available(0)).then_some(c.length(0) as usize)
            })
            .await;

        // The hardware is done writing the buffer
        compiler_fence(Ordering::Acquire);

        debug!("completed OUT to {:02x}: {} bytes", ep, len);

        (buf, len)
    }
}

pub struct Endpoint0 {
    pub(crate) usb: UsbShared
}

impl Endpoint0 {
    pub fn ep0_transfer_in<'b>(&'b mut self, data: &'b [u8], is_full: bool) -> impl Future<Output = ()> + 'b {
        unsafe {
            self.usb.transfer_in(0x80, data.as_ptr(), data.len(), !is_full)
        }
    }
    
    pub async fn ep0_transfer_out(&mut self) -> &[u8] {
        unsafe {
            let (buf, len) = self.usb.out_packet(0).await;
            slice::from_raw_parts(buf, len)
        }
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
fn USBCTRL_IRQ() {
    let flags = USB.ints().read();

    let buf_status = USB.buff_status().read();
    USB.buff_status().write_value(buf_status);

    defmt::trace!("usb irq: flags {:08x} buf_status {:08x}", flags.0, buf_status.0);

    if flags.bus_reset() || flags.setup_req() {
        unsafe { NOTIFY_BUS_EVENT.get_unchecked().notify() };
    }

    for ep in 0..EP_COUNT {
        if buf_status.ep_out(ep) {
            if ep > 0 { defmt::trace!("wake ep{} OUT", ep) };
            unsafe { NOTIFY_EP_OUT.get_unchecked()[ep as usize].notify() };
        }

        if buf_status.ep_in(ep) {
            if ep > 0 { defmt::trace!("wake ep{} IN", ep) };
            unsafe { NOTIFY_EP_IN.get_unchecked()[ep as usize].notify() };
        }
    }
}
