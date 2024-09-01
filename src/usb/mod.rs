use core::{
    cell::Cell,
    future::Future,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    pin::{pin, Pin},
    task::{Context, Poll},
};

use defmt::{debug, panic, write, Format};
use futures_util::{future::FusedFuture, select_biased, FutureExt};
use pin_project_lite::pin_project;
use usb::endpoint_address::{DIR_MASK as EP_DIR_MASK, IN as EP_IN, OUT as EP_OUT};

pub mod descriptors;
use descriptors::DescriptorBuilder;

use crate::executor::TaskOnly;
#[cfg(any(feature = "samd11", feature = "samd21"))]
use crate::samd::usb::{UsbShared, CONTROL_BUF};

#[cfg(any(feature = "samd11", feature = "samd21"))]
pub use crate::samd::usb::Usb;

#[repr(C, align(4))]
pub struct UsbBuffer<const SIZE: usize>([u8; SIZE]);

impl<const SIZE: usize> UsbBuffer<SIZE> {
    pub const fn new() -> Self {
        UsbBuffer([0; SIZE])
    }
}

impl<const SIZE: usize> Deref for UsbBuffer<SIZE> {
    type Target = [u8; SIZE];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const SIZE: usize> DerefMut for UsbBuffer<SIZE> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub(crate) enum Event {
    Reset,
}

/// Specification defining the request.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Format)]
#[repr(u8)]
pub enum ControlType {
    /// Request defined by the USB standard.
    Standard = 0,

    /// Request defined by the standard USB class specification.
    Class = 1,

    /// Non-standard request.
    Vendor = 2,
}

/// Entity targeted by the request.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Format)]
#[repr(u8)]
pub enum Recipient {
    /// Request made to device as a whole.
    Device = 0,

    /// Request made to specific interface.
    Interface = 1,

    /// Request made to specific endpoint.
    Endpoint = 2,

    /// Other request.
    Other = 3,
}

/// Token used to ensure that [`Handler::handle_control`] either rejects or
/// accepts every request.
pub struct Responded {}

pub struct ControlIn<'a> {
    usb: UsbShared,
    length: u16,
    _lt: PhantomData<&'a UsbShared>,
}

pub struct ControlOut<'a> {
    usb: UsbShared,
    length: u16,
    _lt: PhantomData<&'a UsbShared>,
}

pub enum ControlData<'a> {
    In(ControlIn<'a>),
    Out(ControlOut<'a>),
}

impl<'a> Format for ControlData<'a> {
    fn format(&self, f: defmt::Formatter) {
        match self {
            ControlData::In(d) => write!(f, "{} bytes IN", d.length),
            ControlData::Out(d) => write!(f, "{} bytes OUT", d.length),
        }
    }
}

impl<'a> ControlIn<'a> {
    pub fn reject(self) -> Responded {
        debug!("reject in request");
        self.usb.stall_ep0();
        Responded {}
    }

    pub async fn respond(self, data: &[u8]) -> Responded {
        debug!("accepting IN request with {} bytes", data.len());

        // Limit response size to host's request size
        let is_full = data.len() >= self.length as usize;
        let mut data = &data[..data.len().min(self.length as usize)];

        loop {
            // We want to be able to send an arbitrary slice, which may not be
            // correctly aligned or in RAM (USB DMA can't read from flash), so copy
            // a packet at a time to CONTROL_BUF
            let buf = unsafe { &mut CONTROL_BUF.0 };

            let (pkt, remaining) = data.split_at(data.len().min(buf.len()));

            buf[..pkt.len()].copy_from_slice(pkt);
            self.usb
                .transfer_in(0x80, buf.as_ptr(), pkt.len(), false)
                .await;

            if pkt.len() < 64 || (remaining.len() == 0 && is_full) {
                break;
            }

            data = remaining;
        }

        debug!("data phase complete");
        let buf = unsafe { &mut CONTROL_BUF.0 };
        self.usb.transfer_out(0, buf.as_mut_ptr(), 64).await;
        debug!("status phase complete");
        Responded {}
    }
}

impl<'a> ControlOut<'a> {
    pub fn reject(self) -> Responded {
        debug!("reject out request");
        self.usb.stall_ep0();
        Responded {}
    }

    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub async fn accept(&self) -> Responded {
        debug!("accept OUT request");
        let buf = unsafe { &mut CONTROL_BUF.0 };
        //self.usb.transfer_out(0, PacketSize::Size64, buf.as_mut_ptr(), 64).await;
        //debug!("data stage complete");
        self.usb.transfer_in(0x80, buf.as_ptr(), 0, false).await;
        debug!("status stage complete");
        Responded {}
    }
}

pub struct Setup<'a> {
    pub data: ControlData<'a>,

    /// Request type used for the `bmRequestType` field sent in the SETUP packet.
    #[doc(alias = "bmRequestType")]
    pub ty: ControlType,

    /// Recipient used for the `bmRequestType` field sent in the SETUP packet.
    #[doc(alias = "bmRequestType")]
    pub recipient: Recipient,

    /// `bRequest` field sent in the SETUP packet.
    #[doc(alias = "bRequest")]
    pub request: u8,

    /// `wValue` field sent in the SETUP packet.
    #[doc(alias = "wValue")]
    pub value: u16,

    /// `wIndex` field sent in the SETUP packet.
    ///
    /// For [`Recipient::Interface`] this is the interface number. For [`Recipient::Endpoint`] this is the endpoint number.
    #[doc(alias = "wIndex")]
    pub index: u16,
}

impl<'a> Setup<'a> {
    fn parse(usb: UsbShared, packet: [u8; 8]) -> Result<Setup<'a>, ()> {
        use usb::request_type::{
            direction, recipient, request_type, DIRECTION_MASK, RECIPIENT_MASK, REQUEST_TYPE_MASK,
        };
        Ok(Setup {
            recipient: match packet[0] & RECIPIENT_MASK {
                recipient::DEVICE => Recipient::Device,
                recipient::INTERFACE => Recipient::Interface,
                recipient::ENDPOINT => Recipient::Endpoint,
                recipient::OTHER => Recipient::Other,
                _ => return Err(()),
            },
            ty: match packet[0] & REQUEST_TYPE_MASK {
                request_type::STANDARD => ControlType::Standard,
                request_type::CLASS => ControlType::Class,
                request_type::VENDOR => ControlType::Vendor,
                _ => return Err(()),
            },
            request: packet[1],
            value: u16::from_le_bytes([packet[2], packet[3]]),
            index: u16::from_le_bytes([packet[4], packet[5]]),
            data: {
                let length = u16::from_le_bytes([packet[6], packet[7]]);
                if packet[0] & DIRECTION_MASK == direction::OUT {
                    ControlData::Out(ControlOut {
                        length,
                        usb,
                        _lt: PhantomData,
                    })
                } else {
                    ControlData::In(ControlIn {
                        length,
                        usb,
                        _lt: PhantomData,
                    })
                }
            },
        })
    }

    pub fn reject(self) -> Responded {
        match self.data {
            ControlData::In(d) => d.reject(),
            ControlData::Out(d) => d.reject(),
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait Handler {
    async fn handle_reset(&self) {
        debug!("usb reset");
    }

    fn get_descriptor<'a>(&self, _kind: u8, _index: u8, _lang: u16, _builder: &'a mut DescriptorBuilder) -> Option<&'a [u8]> {
        None
    }

    async fn set_configuration(&self, cfg: u8, endpoints: &mut Endpoints) -> Result<(), ()> {
        let _ = endpoints;
        if cfg == 1 {
            Ok(())
        } else {
            Err(())
        }
    }

    async fn set_interface(&self, intf: u8, alt: u8, endpoints: &mut Endpoints) -> Result<(), ()> {
        let _ = (intf, alt, endpoints);
        Err(())
    }

    async fn handle_control<'a>(&self, req: Setup<'a>) -> Responded {
        req.reject()
    }
}

impl Usb {
    /// Attach as a USB device and handle requests using the provided callback.
    ///
    /// If this future is dropped, the device will disconnect.
    pub async fn run_device(&mut self, h: impl Handler) -> ! {
        self.enable();
        self.attach();

        let device = scopeguard::guard(self, |device| {
            device.detach();
        });

        pin_project! {
            #[project = StatePin]
            enum State<F1, F2> {
                Idle,
                Reset{ #[pin] f: F1 },
                Control{ #[pin] f: F2 },
            }
        }

        impl<F1: Future<Output = ()>, F2: Future<Output = ()>> Future for State<F1, F2> {
            type Output = ();

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let done = match self.as_mut().project() {
                    StatePin::Idle => Poll::Pending,
                    StatePin::Reset { f } => f.poll(cx),
                    StatePin::Control { f } => f.poll(cx),
                }
                .is_ready();

                if done {
                    self.set(State::Idle);
                }

                Poll::Pending
            }
        }

        impl<F1: Future<Output = ()>, F2: Future<Output = ()>> FusedFuture for State<F1, F2> {
            fn is_terminated(&self) -> bool {
                false
            }
        }

        let mut inner = pin!(State::Idle);

        loop {
            select_biased! {
                _ = device.bus_event().fuse() => {
                    device.configure_ep0();
                    inner.set(State::Reset { f: h.handle_reset() });
                }
                setup = device.receive_setup().fuse() => {
                    if let Ok(setup) = Setup::parse(device.shared(), setup) {
                        inner.set(State::Control { f: device.handle_control(setup, &h) });
                    } else {
                        inner.set(State::Idle);
                        device.stall_ep0();
                    }
                },
                _ = inner => {}
            }
        }
    }

    async fn handle_control<'a>(&self, req: Setup<'a>, h: &impl Handler) {
        use usb::standard_request::{
            GET_DESCRIPTOR, GET_STATUS, SET_ADDRESS, SET_CONFIGURATION, SET_INTERFACE,
        };
        use ControlData::*;
        use ControlType::*;
        use Recipient::*;
        debug!(
            "control request: {:?} {:?} {:02x} {:04x} {:04x} {:?}",
            req.ty, req.recipient, req.request, req.value, req.index, &req.data
        );

        let Responded {} = match req {
            Setup {
                ty: Standard,
                request: GET_STATUS,
                data: In(data),
                ..
            } => data.respond(&[0, 0]).await,
            Setup {
                ty: Standard,
                recipient: Device,
                request: GET_DESCRIPTOR,
                value,
                index,
                data: In(data),
            } => {
                let lang = index;
                let kind = (value >> 8) as u8;
                let index = (value & 0xFF) as u8;
                let mut builder = DescriptorBuilder::new();
                if let Some(descriptor) = h.get_descriptor(kind, index, lang, &mut builder) {
                    debug!("returning descriptor");
                    data.respond(descriptor).await
                } else {
                    debug!("descriptor not found");
                    data.reject()
                }
            }
            Setup {
                ty: Standard,
                recipient: Device,
                request: SET_ADDRESS,
                value,
                data: Out(data),
                ..
            } => {
                debug!("set address {}", value);
                let r = data.accept().await;
                self.set_address(value as u8);
                r
            }
            Setup {
                ty: Standard,
                recipient: Device,
                request: SET_CONFIGURATION,
                value,
                data: Out(data),
                ..
            } => {
                debug!("set configuration {}", value);
                match h
                    .set_configuration(value as u8, &mut Endpoints { usb: **self })
                    .await
                {
                    Ok(_) => data.accept().await,
                    Err(_) => data.reject(),
                }
            }
            Setup {
                ty: Standard,
                recipient: Interface,
                request: SET_INTERFACE,
                index,
                value,
                data: Out(data),
                ..
            } => {
                debug!("set interface {} {}", index, value);
                match h
                    .set_interface(index as u8, value as u8, &mut Endpoints { usb: **self })
                    .await
                {
                    Ok(_) => data.accept().await,
                    Err(_) => data.reject(),
                }
            }
            other => h.handle_control(other).await,
        };
    }
}

pub struct Endpoints {
    usb: UsbShared,
}

static EP_ENABLED: TaskOnly<Cell<u32>> =
    unsafe { TaskOnly::new(Cell::new(ep_enabled_mask(0 | EP_OUT) | ep_enabled_mask(0 | EP_IN))) };

const fn ep_enabled_mask(ep: u8) -> u32 {
    let bit = ((ep & 0x0f) << 1) | (ep >> 7);
    1 << bit
}

impl Endpoints {
    fn mark_enabled(&self, ep: u8) {
        let mask = ep_enabled_mask(ep);
        let enabled = &EP_ENABLED.get(self.usb.rt());
        if enabled.get() & mask != 0 {
            panic!("Endpoint {:02x} already in use", ep);
        }
        enabled.set(enabled.get() | mask);
    }

    pub fn bulk_in<const EP: u8>(&self) -> Endpoint<In, EP> {
        const {
            assert!(EP & EP_DIR_MASK == EP_IN);
        }
        self.mark_enabled(EP);
        self.usb.enable_ep(EP);
        Endpoint {
            usb: self.usb,
            _d: PhantomData,
        }
    }

    pub fn bulk_out<const EP: u8>(&self) -> Endpoint<Out, EP> {
        const {
            assert!(EP & EP_DIR_MASK == EP_OUT);
        }
        self.mark_enabled(EP);
        self.usb.enable_ep(EP);
        Endpoint {
            usb: self.usb,
            _d: PhantomData,
        }
    }
}

pub struct In;
pub struct Out;

pub struct Endpoint<D, const EP: u8> {
    usb: UsbShared,
    _d: PhantomData<D>,
}

impl<const EP: u8> Endpoint<Out, EP> {
    pub async fn receive<const SIZE: usize>(&mut self, buf: &mut UsbBuffer<SIZE>) -> usize {
        assert!(SIZE >= 64);
        self.usb.transfer_out(EP, buf.as_mut_ptr(), buf.len()).await
    }
}

impl<const EP: u8> Endpoint<In, EP> {
    pub async fn send<const SIZE: usize>(&mut self, buf: &UsbBuffer<SIZE>, len: usize, zlp: bool) {
        assert!(len <= SIZE);
        self.usb.transfer_in(EP, buf.as_ptr(), len, zlp).await
    }
}

impl<D, const EP: u8> Drop for Endpoint<D, EP> {
    fn drop(&mut self) {
        self.usb.disable_ep(EP);
        let enabled = &EP_ENABLED.get(self.usb.rt());
        let mask = ep_enabled_mask(EP);
        enabled.set(enabled.get() & !mask);
    }
}
