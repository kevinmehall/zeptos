#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use defmt_rtt as _;
use panic_probe as _;

use core::pin::pin;
use core::cell::{ Cell, RefCell };

use defmt::info;
use futures_util::future;

use zeptos::rp::gpio::{self, TypePin, Function};
use zeptos::{
    usb::descriptors::{DescriptorBuilder, LANGUAGE_LIST_US_ENGLISH, MicrosoftOsCompatibleID, MicrosoftOs, BinaryObjectStore, PlatformCapabilityMicrosoftOs},
    usb::{Endpoint, Endpoints, In, Out, Responded, Setup, UsbBuffer},
    Hardware, Runtime,
};

#[cfg(feature = "msos_composite")]
use zeptos::usb::descriptors::{MicrosoftOsCcgp, MicrosoftOsConfiguration, MicrosoftOsFunction, MicrosoftOsDeviceInterfaceGUID};

#[zeptos::main]
async fn main(rt: Runtime, mut hw: Hardware) {
    info!("init");
    hw.usb.run_device(&mut ExampleDevice { rt, count: Cell::new(0), echo_payload: RefCell::new([0; 16]) }).await;
}

const REQ_COUNT: u8 = 0x01;
const REQ_SLOW: u8 = 0x02;
const REQ_ECHO: u8 = 0x03;

struct ExampleDevice {
    rt: Runtime,
    count: Cell<u32>,
    echo_payload: RefCell<[u8; 16]>,
}

impl zeptos::usb::Handler for ExampleDevice {
    fn get_descriptor<'a>(
        &self,
        kind: u8,
        index: u8,
        _lang: u16,
        builder: &'a mut DescriptorBuilder,
    ) -> Option<&'a [u8]> {
        use usb::descriptor_type::{CONFIGURATION, DEVICE, STRING, BOS};
        match (kind, index) {
            (DEVICE, _) => Some(DEVICE_DESCRIPTOR),
            (CONFIGURATION, 0) => Some(CONFIG_DESCRIPTOR),
            (BOS, 0) => Some(BOS_DESCRIPTOR),
            (STRING, 0) => Some(LANGUAGE_LIST_US_ENGLISH),
            (STRING, STRING_MFG) => Some(builder.string_ascii("zeptos project")),
            (STRING, STRING_PRODUCT) => Some(builder.string_ascii("rp2040 test device")),
            (STRING, STRING_SERIAL) => Some(builder.string_hex(&zeptos::serial_number())),
            (STRING, STRING_INTF_MAIN) => Some(builder.string_ascii("Test Interface")),
            (STRING, STRING_INTF_ECHO) => Some(builder.string_ascii("Echo Interface")),
            _ => None,
        }
    }

    async fn set_configuration(&self, cfg: u8, endpoints: &mut Endpoints) -> Result<(), ()> {
        match cfg {
            0 => {
                self.unconfigure_main();
                self.unconfigure_echo();
                Ok(())
            }
            CFG_MAIN => {
                self.configure_main(endpoints);
                self.unconfigure_echo();
                Ok(())
            }
            _ => Err(()),
        }
    }

    async fn set_interface(&self, intf: u8, alt: u8, endpoints: &mut Endpoints) -> Result<(), ()> {
        match (intf, alt) {
            (INTF_MAIN, 0) => {
                self.configure_main(endpoints);
                Ok(())
            }
            (INTF_ECHO, 0) => {
                self.unconfigure_echo();
                Ok(())
            }
            (INTF_ECHO, 1) => {
                self.configure_echo(endpoints);
                Ok(())
            }
            _ => Err(()),
        }
    }

    async fn handle_control<'a>(&self, req: Setup<'a>) -> Responded {
        use zeptos::usb::ControlData::*;
        use zeptos::usb::ControlType::*;
        use zeptos::usb::Recipient::*;

        match req {
            Setup { ty: Vendor, recipient: Device, request: MSOS_VENDOR_CODE, index: 0x07, data: In(data), .. } => {
                data.respond(&MSOS_DESCRIPTOR).await
            }
            Setup { ty: Vendor, recipient: Device, request: REQ_COUNT, value: _, index: _, data: In(data) } => {
                self.count.set(self.count.get() + 1);
                data.respond(&self.count.get().to_le_bytes()).await
            }
            Setup { ty: Vendor, recipient: Device, request: REQ_ECHO, value, index, data: Out(mut data) } => {
                let mut echo = self.echo_payload.borrow_mut();
                let d = data.receive().await;
                let len = d.len().min(12);
                echo[0..2].copy_from_slice(&value.to_le_bytes());
                echo[2..4].copy_from_slice(&index.to_le_bytes());
                echo[4..4 + len].copy_from_slice(&d[..len]);
                if data.remaining() != 0 {
                    return data.reject();
                }
                data.accept().await
            }
            Setup { ty: Vendor, recipient: Device, request: REQ_ECHO, value: _, index: _, data: In(data) } => {
                let echo = self.echo_payload.borrow();
                data.respond(&echo[..]).await
            }
            Setup { ty: Vendor, recipient: Device, request: REQ_SLOW, value, index: _, data } => {
                self.rt.delay_us(value as u32 * 8).await;
                match data {
                    In(data) => data.respond(&[]).await,
                    Out(data) => data.accept().await,
                }
            }
            req => req.reject(),
        }
    }
}

impl ExampleDevice {
    fn configure_main(&self, endpoints: &mut Endpoints) {
        self.unconfigure_main();
        info!("Configure main interface");

        let ep_stream_in = endpoints.bulk_in::<EP_STREAM_IN>();
        stream_in_task(self.rt).spawn(ep_stream_in);

        let ep_stream_out = endpoints.bulk_out::<EP_STREAM_OUT>();
        stream_out_task(self.rt).spawn(ep_stream_out);

        let ep_int_in = endpoints.interrupt_in::<EP_INT_IN>();
        periodic_task(self.rt).spawn(self.rt, ep_int_in);
    }

    fn unconfigure_main(&self) {
        info!("Unconfigure main interface");
        stream_in_task(self.rt).cancel();
        stream_out_task(self.rt).cancel();
        periodic_task(self.rt).cancel();
    }

    fn configure_echo(&self, endpoints: &mut Endpoints) {
        self.unconfigure_echo();
        info!("Configure echo interface");
        let ep_echo_out = endpoints.bulk_out::<EP_ECHO_OUT>();
        let ep_echo_in = endpoints.bulk_in::<EP_ECHO_IN>();
        echo_task(self.rt).spawn(ep_echo_out, ep_echo_in);
    }

    fn unconfigure_echo(&self) {
        info!("Unconfigure echo interface");
        echo_task(self.rt).cancel();
    }
}

const EP_ECHO_OUT: u8 = 0x01;
const EP_ECHO_IN: u8 = 0x81;
const EP_INT_IN: u8 = 0x82;
const EP_STREAM_IN: u8 = 0x83;
const EP_STREAM_OUT: u8 = 0x03;

#[zeptos::task]
async fn stream_in_task(mut ep_in: Endpoint<In, EP_STREAM_IN>) {
    let mut count: u32 = 0;
    let mut buf = UsbBuffer::<64>::new();
    loop {
        buf[0..4].copy_from_slice(&count.to_le_bytes());
        ep_in.send(&buf, buf.len(), false).await;
        count = count.wrapping_add(1);
    }
}

#[zeptos::task]
async fn stream_out_task(mut ep_in: Endpoint<Out, EP_STREAM_OUT>) {
    let mut buf = UsbBuffer::<64>::new();
    loop {
        ep_in.receive(&mut buf).await;
    }
}


#[zeptos::task]
async fn echo_task(mut ep_out: Endpoint<Out, EP_ECHO_OUT>, mut ep_in: Endpoint<In, EP_ECHO_IN>) {
    let mut buf = UsbBuffer::<64>::new();
    loop {
        let len = ep_out.receive(&mut buf).await;
        ep_in.send(&buf, len, false).await;
    }
}

#[zeptos::task]
async fn periodic_task(rt: Runtime, mut ep_int_in: Endpoint<In, EP_INT_IN>) {
    let mut count: u32 = 0;
    let mut buf = UsbBuffer::<64>::new();
    gpio::GPIO25::set_function(Function::F5);
    gpio::GPIO25::oe_set();

    loop {
        buf[0..4].copy_from_slice(&count.to_le_bytes());
        buf[4..8].copy_from_slice(&rt.now().0.to_le_bytes());
        
        let time = pin!(async {
            rt.delay_us(150_000).await;
            gpio::GPIO25::out_set();
            rt.delay_us(100_000).await;
            gpio::GPIO25::out_clr();
        });

        let send = pin!(async {
            ep_int_in.send(&buf, 8, false).await;
            future::pending::<()>().await;
        });

        future::select(time, send).await;
        
        defmt::info!("blink");
        count = count.wrapping_add(1);
    }
}

use zeptos::usb::descriptors::{
    descriptors, Config, Device, Endpoint as EndpointDescriptor, Interface,
};

const CFG_MAIN: u8 = 1;
const INTF_MAIN: u8 = 0;
const INTF_ECHO: u8 = 1;

const STRING_MFG: u8 = 1;
const STRING_PRODUCT: u8 = 2;
const STRING_SERIAL: u8 = 3;
const STRING_INTF_MAIN: u8 = 4;
const STRING_INTF_ECHO: u8 = 5;

static DEVICE_DESCRIPTOR: &[u8] = descriptors! {
    Device {
        bcdUSB: 0x0201,
        bDeviceClass: ::usb::class_code::VENDOR_SPECIFIC,
        bDeviceSubClass: 0x00,
        bDeviceProtocol: 0x00,
        bMaxPacketSize0: 64,
        idVendor: 0x59e3,
        idProduct: 0x00AA,
        bcdDevice: const { if cfg!(feature = "msos_composite") { 0x0001 } else { 0x0000 } },
        iManufacturer: STRING_MFG,
        iProduct: STRING_PRODUCT,
        iSerialNumber: STRING_SERIAL,
        bNumConfigurations: 1,
    }
};

static CONFIG_DESCRIPTOR: &[u8] = descriptors! {
    Config {
        bConfigurationValue: CFG_MAIN,
        iConfiguration: 0,
        bmAttributes: 0x80,
        bMaxPower: 250,

        +Interface {
            bInterfaceNumber: INTF_MAIN,
            bAlternateSetting: 0,
            bInterfaceClass: usb::class_code::VENDOR_SPECIFIC,
            bInterfaceSubClass: 0,
            bInterfaceProtocol: 0,
            iInterface: STRING_INTF_MAIN,

            +EndpointDescriptor {
                bEndpointAddress: EP_INT_IN,
                bmAttributes: usb::endpoint_attributes::transfer_type::INTERRUPT,
                wMaxPacketSize: 64,
                bInterval: 10,
            }

            +EndpointDescriptor {
                bEndpointAddress: EP_STREAM_IN,
                bmAttributes: usb::endpoint_attributes::transfer_type::BULK,
                wMaxPacketSize: 64,
                bInterval: 0,
            }

            +EndpointDescriptor {
                bEndpointAddress: EP_STREAM_OUT,
                bmAttributes: usb::endpoint_attributes::transfer_type::BULK,
                wMaxPacketSize: 64,
                bInterval: 0,
            }
        }

        +Interface {
            bInterfaceNumber: INTF_ECHO,
            bAlternateSetting: 0,
            bInterfaceClass: usb::class_code::VENDOR_SPECIFIC,
            bInterfaceSubClass: 0,
            bInterfaceProtocol: 0,
            iInterface: STRING_INTF_ECHO,
        }

        +Interface {
            bInterfaceNumber: INTF_ECHO,
            bAlternateSetting: 1,
            bInterfaceClass: usb::class_code::VENDOR_SPECIFIC,
            bInterfaceSubClass: 0,
            bInterfaceProtocol: 0,
            iInterface: STRING_INTF_ECHO,

            +EndpointDescriptor {
                bEndpointAddress: EP_ECHO_OUT,
                bmAttributes: usb::endpoint_attributes::transfer_type::BULK,
                wMaxPacketSize: 64,
                bInterval: 0,
            }

            +EndpointDescriptor {
                bEndpointAddress: EP_ECHO_IN,
                bmAttributes: usb::endpoint_attributes::transfer_type::BULK,
                wMaxPacketSize: 64,
                bInterval: 0,
            }
        }
    }
};

#[cfg(not(feature = "msos_composite"))]
const MSOS_DESCRIPTOR: &[u8] = descriptors!{
    MicrosoftOs {
        windows_version: 0x06030000,
        
        +MicrosoftOsCompatibleID {
            compatible_id: "WINUSB",
            sub_compatible_id: "",
        }
    }
};

#[cfg(feature = "msos_composite")]
const MSOS_DESCRIPTOR: &[u8] = descriptors!{
    MicrosoftOs {
        windows_version: 0x06030000,

        +MicrosoftOsCcgp {}

        +MicrosoftOsConfiguration {
            configuration_value: 0,

            +MicrosoftOsFunction {
                first_interface: 0,

                +MicrosoftOsCompatibleID {
                    compatible_id: "WINUSB",
                    sub_compatible_id: "",
                }

                +MicrosoftOsDeviceInterfaceGUID {
                    guid: "{420F4791-4A3B-40A6-B8E9-4F63EF6017B9}",
                }
            }
        }
    }
};

pub const MSOS_VENDOR_CODE: u8 = 0xf0;

pub static BOS_DESCRIPTOR: &[u8] = descriptors!{
    BinaryObjectStore {
        +PlatformCapabilityMicrosoftOs {
            windows_version: 0x06030000,
            vendor_code: MSOS_VENDOR_CODE,
            alt_enum_code: 0,
            msos_descriptor_len: MSOS_DESCRIPTOR.len(),
        }
    }
};

