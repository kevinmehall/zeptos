use core::{
    cell::Cell, convert::Infallible, future::Future, marker::PhantomData, ops::{Deref, DerefMut}, pin::{pin, Pin}, task::{Context, Poll}
};

use defmt::{debug, error, panic, write, Format};
use pin_project::{pin_project, pinned_drop};
use usb::endpoint_address::{DIR_MASK as EP_DIR_MASK, IN as EP_IN, OUT as EP_OUT};

pub mod descriptors;
use descriptors::DescriptorBuilder;

use crate::executor::TaskOnly;

#[cfg(any(feature = "samd11", feature = "samd21"))]
pub use crate::samd::usb::{ Usb, UsbShared, Endpoint0 };

#[cfg(any(feature = "rp2040"))]
pub use crate::rp::usb::{ Usb, UsbShared, Endpoint0 };

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

pub enum Event {
    Reset,
    Setup([u8; 8]),
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
    usb: Endpoint0,
    length: u16,
    _lt: PhantomData<&'a UsbShared>,
}

pub struct ControlOut<'a> {
    usb: Endpoint0,
    length: u16,
    remaining: u16,
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
    pub fn reject(mut self) -> Responded {
        debug!("reject in request");
        self.usb.stall_ep0();
        Responded {}
    }

    pub async fn respond(mut self, data: &[u8]) -> Responded {
        debug!("accepting IN request with {} bytes", data.len());

        // Limit response size to host's request size
        let is_full = data.len() >= self.length as usize;
        let data = &data[..data.len().min(self.length as usize)];

        self.usb.ep0_transfer_in(data, is_full).await;

        debug!("data phase complete");

        self.usb.ep0_transfer_out().await;

        debug!("status phase complete");
        Responded {}
    }
}

impl<'a> ControlOut<'a> {
    pub fn reject(mut self) -> Responded {
        debug!("reject out request");
        self.usb.stall_ep0();
        Responded {}
    }

    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub fn remaining(&self) -> usize {
        self.remaining as usize
    }

    pub async fn receive(&mut self) -> &[u8] {
        if self.remaining > 0 {
            let data = self.usb.ep0_transfer_out().await;
            self.remaining = self.remaining.saturating_sub(data.len() as u16);
            data
        } else {
            &[]
        }
    }

    pub async fn accept(mut self) -> Responded {
        debug_assert!(self.remaining == 0);
        debug!("accept OUT request");
        self.usb.ep0_transfer_in(&[], true).await;
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
    fn parse(usb: Endpoint0, packet: [u8; 8]) -> Result<Setup<'a>, ()> {
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
                        remaining: length,
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

    pub fn usb(&self) -> UsbShared {
        match &self.data {
            ControlData::In(c) => c.usb.usb,
            ControlData::Out(c) => c.usb.usb,
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait Handler {
    fn handle_reset(&self) {
        debug!("usb reset");
    }

    async fn handle_control_raw<'a>(&self, req: Setup<'a>) {
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

        let usb = req.usb();

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
                if let Some(descriptor) = self.get_descriptor(kind, index, lang, &mut builder) {
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
                usb.set_address(value as u8);
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
                match self
                    .set_configuration(value as u8, &mut Endpoints { usb })
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
                match self
                    .set_interface(index as u8, value as u8, &mut Endpoints { usb })
                    .await
                {
                    Ok(_) => data.accept().await,
                    Err(_) => data.reject(),
                }
            }
            other => self.handle_control(other).await,
        };
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

pub trait HandlerFut: Handler {
    type ControlRawFut<'a>: Future<Output = ()> + 'a where Self: 'a;

    fn handle_control_raw_fut<'a>(&'a self, req: Setup<'a>) -> Self::ControlRawFut<'a>;
}

impl<T: Handler> HandlerFut for T {
    type ControlRawFut<'a>: = impl Future<Output = ()> + 'a where T: 'a;

    fn handle_control_raw_fut<'a>(&'a self, req: Setup<'a>) -> Self::ControlRawFut<'a> {
        self.handle_control_raw(req)
    }
}

impl Usb {
    /// Attach as a USB device and handle requests using the provided callback.
    ///
    /// If this future is dropped, the device will disconnect.
    pub fn run_device<'a>(&'a mut self, handler: &'a mut impl Handler) -> impl Future<Output = Infallible> + 'a {
        self.enable();
        self.attach();

        #[pin_project(PinnedDrop)]
        struct Fut<'a, H: HandlerFut + 'a> {
            usb: &'a mut Usb,

            handler: &'a H,

            #[pin]
            state: Option<H::ControlRawFut<'a>>,
        }

        impl<'a, H: Handler> Future for Fut<'a, H> {
            type Output = Infallible;
        
            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut this = self.project();
                match this.usb.poll_event(cx) {
                    Poll::Ready(Event::Reset) => {
                        this.state.set(None);
                        this.usb.configure_ep0();
                        this.handler.handle_reset();
                    }

                    Poll::Ready(Event::Setup(setup)) => {
                        this.state.set(None);
                        if let Ok(setup) = Setup::parse(unsafe { this.usb.ep0() }, setup) {
                            this.state.set(Some(this.handler.handle_control_raw_fut(setup)));
                        } else {
                            error!("invalid setup packet: {:x}", setup);
                            this.usb.stall_ep0();
                        }
                    }

                    Poll::Pending => {}
                }

                let done = match this.state.as_mut().as_pin_mut() {
                    Some(fut) => fut.poll(cx),
                    None => Poll::Pending,
                }.is_ready();

                if done {
                    this.state.set(None);
                }

                Poll::Pending
            }
        }

        #[pinned_drop]
        impl<'a, H: Handler> PinnedDrop for Fut<'a, H> {
            fn drop(self: Pin<&mut Self>) {
                let this = self.project();
                this.usb.detach();
            }
        }

        Fut {
            usb: self,
            state: None,
            handler,
        }
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

    fn bulk_interrupt<D: EpDir, const EP: u8>(&self) -> Endpoint<D, EP> {
        const {
            assert!(EP & EP_DIR_MASK == D::DIR);
        }
        self.mark_enabled(EP);
        self.usb.enable_ep(EP);
        Endpoint {
            usb: self.usb,
            _d: PhantomData,
        }
    }

    pub fn bulk_in<const EP: u8>(&self) -> Endpoint<In, EP> {
        self.bulk_interrupt()
    }

    pub fn bulk_out<const EP: u8>(&self) -> Endpoint<Out, EP> {
        self.bulk_interrupt()
    }

    pub fn interrupt_in<const EP: u8>(&self) -> Endpoint<In, EP> {
        self.bulk_interrupt()
    }

    pub fn interrupt_out<const EP: u8>(&self) -> Endpoint<Out, EP> {
        self.bulk_interrupt()
    }
}


pub trait EpDir {
    const DIR: u8;
}
pub struct In;
pub struct Out;

impl EpDir for In {
    const DIR: u8 = EP_IN;
}
impl EpDir for Out {
    const DIR: u8 = EP_OUT;
}

pub struct Endpoint<D, const EP: u8> {
    usb: UsbShared,
    _d: PhantomData<D>,
}

impl<const EP: u8> Endpoint<Out, EP> {
    pub fn receive<const SIZE: usize>(&mut self, buf: &mut UsbBuffer<SIZE>) -> impl Future<Output = usize> + '_ {
        assert!(SIZE >= 64);
        unsafe { self.usb.transfer_out(EP, buf.as_mut_ptr(), buf.len()) }
    }
}

impl<const EP: u8> Endpoint<In, EP> {
    pub fn send<const SIZE: usize>(&mut self, buf: &UsbBuffer<SIZE>, len: usize, zlp: bool) -> impl Future<Output = ()> + '_ {
        assert!(len <= SIZE);
        unsafe { self.usb.transfer_in(EP, buf.as_ptr(), len, zlp) }
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
