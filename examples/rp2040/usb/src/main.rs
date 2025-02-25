#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]

use defmt_rtt as _;
use panic_probe as _;

use defmt::info;

use zeptos::rp::{
    gpio::{self, TypePin, Function},
    //serial_number,
};
use zeptos::{
    cortex_m::SysTick,
    usb::descriptors::{DescriptorBuilder, LANGUAGE_LIST_US_ENGLISH},
    usb::{Endpoint, Endpoints, In, Out, Responded, Setup, UsbBuffer},
    Hardware, Runtime,
};

#[zeptos::main]
async fn main(rt: Runtime, mut hw: Hardware) {
    info!("init");
    led_task(rt).spawn(hw.syst);
    hw.usb.run_device(&mut ExampleDevice { rt }).await;
}

struct ExampleDevice {
    rt: Runtime,
}
impl zeptos::usb::Handler for ExampleDevice {
    fn get_descriptor<'a>(
        &self,
        kind: u8,
        index: u8,
        _lang: u16,
        builder: &'a mut DescriptorBuilder,
    ) -> Option<&'a [u8]> {
        use usb::descriptor_type::{CONFIGURATION, DEVICE, STRING};
        match (kind, index) {
            (DEVICE, _) => Some(DEVICE_DESCRIPTOR),
            (CONFIGURATION, 0) => Some(CONFIG_DESCRIPTOR),
            (STRING, 0) => Some(LANGUAGE_LIST_US_ENGLISH),
            (STRING, STRING_MFG) => Some(builder.string_ascii("zeptos project")),
            (STRING, STRING_PRODUCT) => Some(builder.string_ascii("rp2040 test device")),
            (STRING, STRING_SERIAL) => Some(builder.string_hex(&zeptos::rp::serial_number())),
            _ => None,
        }
    }

    async fn set_configuration(&self, cfg: u8, endpoints: &mut Endpoints) -> Result<(), ()> {
        match cfg {
            0 => {
                self.unconfigure();
                Ok(())
            }
            CFG_MAIN => {
                self.configure(endpoints);
                Ok(())
            }
            _ => Err(()),
        }
    }

    async fn set_interface(&self, intf: u8, alt: u8, endpoints: &mut Endpoints) -> Result<(), ()> {
        match (intf, alt) {
            (INTF_MAIN, 0) => {
                self.configure(endpoints);
                Ok(())
            }
            _ => Err(()),
        }
    }

    async fn handle_control<'a>(&self, req: Setup<'a>) -> Responded {
        match req {
            unknown => unknown.reject(),
        }
    }
}

impl ExampleDevice {
    fn configure(&self, endpoints: &mut Endpoints) {
        self.unconfigure();
        info!("Configure");
        let ep_out = endpoints.bulk_out::<EP_OUT>();
        let ep_in = endpoints.bulk_in::<EP_IN>();
        bulk_task(self.rt).spawn(ep_out, ep_in);
    }

    fn unconfigure(&self) {
        info!("Unconfigure");
        bulk_task(self.rt).cancel();
    }
}

const EP_OUT: u8 = 0x01;
const EP_IN: u8 = 0x81;

#[zeptos::task]
async fn bulk_task(mut ep_out: Endpoint<Out, EP_OUT>, mut ep_in: Endpoint<In, EP_IN>) {
    let mut buf = UsbBuffer::<64>::new();
    loop {
        let len = ep_out.receive(&mut buf).await;
        ep_in.send(&buf, len, false).await;
    }
}

#[zeptos::task]
async fn led_task(mut syst: SysTick) {
    gpio::GPIO25::set_function(Function::F5);
    gpio::GPIO25::oe_set();

    loop {
        for _ in 0..10 { syst.delay(16_000_000).await; }
        gpio::GPIO25::out_set();
        for _ in 0..10 { syst.delay(16_000_000).await; }
        gpio::GPIO25::out_clr();
        defmt::info!("blink");
    }
}

use zeptos::usb::descriptors::{
    descriptors, Config, Device, Endpoint as EndpointDescriptor, Interface,
};

const CFG_MAIN: u8 = 1;
const INTF_MAIN: u8 = 0;

const STRING_MFG: u8 = 1;
const STRING_PRODUCT: u8 = 2;
const STRING_SERIAL: u8 = 3;

static DEVICE_DESCRIPTOR: &[u8] = descriptors! {
    Device {
        bcdUSB: 0x0200,
        bDeviceClass: ::usb::class_code::VENDOR_SPECIFIC,
        bDeviceSubClass: 0x00,
        bDeviceProtocol: 0x00,
        bMaxPacketSize0: 64,
        idVendor: 0x59e3,
        idProduct: 0x2222,
        bcdDevice: 0x0000,
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
            iInterface: 0,

            +EndpointDescriptor {
                bEndpointAddress: EP_OUT,
                bmAttributes: usb::endpoint_attributes::transfer_type::BULK,
                wMaxPacketSize: 64,
                bInterval: 0,
            }

            +EndpointDescriptor {
                bEndpointAddress: EP_IN,
                bmAttributes: usb::endpoint_attributes::transfer_type::BULK,
                wMaxPacketSize: 64,
                bInterval: 0,
            }
        }
    }
};
