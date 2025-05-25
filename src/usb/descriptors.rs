#[macro_export]
macro_rules! descriptors {
    (
        $struct:path {
            $($field:ident: $value:expr,)*

            $(+$child:path { $($inner:tt)* })*
        }
    ) => {{
        use $struct as Desc; // https://github.com/rust-lang/rust/issues/48067
        const CHILDREN: &[&[u8]] = {
            &[
                $(
                    descriptors!($child { $($inner)* })
                ),*
            ]
        };
        const LEN: usize = {
            let mut i = 0;
            let mut len = Desc::LEN;
            while i < CHILDREN.len() {
                len += CHILDREN[i].len();
                i += 1;
            }
            len
        };
        const ARR: [u8; LEN] = {
            let desc = Desc::bytes(Desc {
                $($field: $value),*
            }, CHILDREN);

            assert!(desc.len() == Desc::LEN);

            let mut bytes = [0u8; LEN];
            
            let (mut first, mut rest) = bytes.split_at_mut(desc.len());
            first.copy_from_slice(&desc);

            let mut child = 0;
            while child < CHILDREN.len() {
                (first, rest) = rest.split_at_mut(CHILDREN[child].len());
                first.copy_from_slice(&CHILDREN[child]);
                child += 1;
            }

            bytes
        };

        &ARR
    }}
}

pub use descriptors;

#[allow(non_snake_case)]
pub struct Device {
    pub bcdUSB: u16,
    pub bDeviceClass: u8,
    pub bDeviceSubClass: u8,
    pub bDeviceProtocol: u8,
    pub bMaxPacketSize0: u8,
    pub idVendor: u16,
    pub idProduct: u16,
    pub bcdDevice: u16,
    pub iManufacturer: u8,
    pub iProduct: u8,
    pub iSerialNumber: u8,
    pub bNumConfigurations: u8,
}

impl Device {
    pub const LEN: usize = 18;
    pub const DESCRIPTOR_TYPE: u8 = usb::descriptor_type::DEVICE;

    pub const fn bytes(self, children: &[&[u8]]) -> [u8; Self::LEN] {
        assert!(children.is_empty());

        [
            Self::LEN as u8,
            Self::DESCRIPTOR_TYPE,
            self.bcdUSB.to_le_bytes()[0],
            self.bcdUSB.to_le_bytes()[1],
            self.bDeviceClass,
            self.bDeviceSubClass,
            self.bDeviceProtocol,
            self.bMaxPacketSize0,
            self.idVendor.to_le_bytes()[0],
            self.idVendor.to_le_bytes()[1],
            self.idProduct.to_le_bytes()[0],
            self.idProduct.to_le_bytes()[1],
            self.bcdDevice.to_le_bytes()[0],
            self.bcdDevice.to_le_bytes()[1],
            self.iManufacturer,
            self.iProduct,
            self.iSerialNumber,
            self.bNumConfigurations,
        ]
    }
}

#[allow(non_snake_case)]
pub struct Config {
    pub bConfigurationValue: u8,
    pub iConfiguration: u8,
    pub bmAttributes: u8,
    pub bMaxPower: u8,
}

impl Config {
    pub const LEN: usize = 9;
    pub const DESCRIPTOR_TYPE: u8 = usb::descriptor_type::CONFIGURATION;

    pub const fn bytes(self, children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut total_len = Self::LEN as u16;
        let mut interface_number = 0;
        let mut num_interfaces = 0;

        let mut i = 0;
        while i < children.len() {
            total_len += children[i].len() as u16;
            if children[i][1] == Interface::DESCRIPTOR_TYPE {
                let intf = children[i][2];
                assert!(
                    intf == interface_number || intf == interface_number + 1,
                    "interface numbers must be contiguous"
                );
                interface_number = intf;
                num_interfaces = intf + 1;
            }
            i += 1;
        }

        [
            Self::LEN as u8,
            Self::DESCRIPTOR_TYPE,
            total_len.to_le_bytes()[0],
            total_len.to_le_bytes()[1],
            num_interfaces,
            self.bConfigurationValue,
            self.iConfiguration,
            self.bmAttributes,
            self.bMaxPower,
        ]
    }
}

/// Fields of a USB interface descriptor.
///
/// The `bLength` and `bDescriptorType` are fixed. `bNumEndpoints` is populated from the
/// number of child endpoint descriptors.
#[allow(non_snake_case)]
pub struct Interface {
    pub bInterfaceNumber: u8,
    pub bAlternateSetting: u8,
    pub bInterfaceClass: u8,
    pub bInterfaceSubClass: u8,
    pub bInterfaceProtocol: u8,
    pub iInterface: u8,
}

impl Interface {
    pub const LEN: usize = 9;
    pub const DESCRIPTOR_TYPE: u8 = usb::descriptor_type::INTERFACE;

    pub const fn bytes(self, children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut num_endpoints = 0;
        let mut i = 0;
        while i < children.len() {
            if children[i][1] == Endpoint::DESCRIPTOR_TYPE {
                num_endpoints += 1;
            }
            i += 1;
        }

        [
            Self::LEN as u8,
            Self::DESCRIPTOR_TYPE,
            self.bInterfaceNumber,
            self.bAlternateSetting,
            num_endpoints as u8,
            self.bInterfaceClass,
            self.bInterfaceSubClass,
            self.bInterfaceProtocol,
            self.iInterface,
        ]
    }
}

#[allow(non_snake_case)]
pub struct Endpoint {
    pub bEndpointAddress: u8,
    pub bmAttributes: u8,
    pub wMaxPacketSize: u16,
    pub bInterval: u8,
}

impl Endpoint {
    pub const LEN: usize = 7;
    pub const DESCRIPTOR_TYPE: u8 = usb::descriptor_type::ENDPOINT;

    pub const fn bytes(self, _children: &[&[u8]]) -> [u8; Self::LEN] {
        [
            Endpoint::LEN as u8,
            Endpoint::DESCRIPTOR_TYPE,
            self.bEndpointAddress,
            self.bmAttributes,
            self.wMaxPacketSize.to_le_bytes()[0],
            self.wMaxPacketSize.to_le_bytes()[1],
            self.bInterval,
        ]
    }
}

pub struct BinaryObjectStore {}

impl BinaryObjectStore {
    pub const LEN: usize = 5;
    pub const DESCRIPTOR_TYPE: u8 = usb::descriptor_type::BOS;

    pub const fn bytes(self, children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut total_len = Self::LEN as u16;
        let mut num_device_caps = 0;

        let mut i = 0;
        while i < children.len() {
            total_len += children[i].len() as u16;
            if children[i][1] == usb::descriptor_type::DEVICE_CAPABILITY {
                num_device_caps += 1;
            }
            i += 1;
        }

        [
            Self::LEN as u8,
            Self::DESCRIPTOR_TYPE,
            total_len.to_le_bytes()[0],
            total_len.to_le_bytes()[1],
            num_device_caps,
        ]
    }
}

const DEVICE_CAPABILITY_TYPE_PLATFORM: u8 = 0x05;

pub struct PlatformCapabilityMicrosoftOs {
    pub windows_version: u32,
    pub vendor_code: u8,
    pub alt_enum_code: u8,
    pub msos_descriptor_len: usize,
}

impl PlatformCapabilityMicrosoftOs {
    pub const LEN: usize = 28;
    pub const DESCRIPTOR_TYPE: u8 = usb::descriptor_type::DEVICE_CAPABILITY;

    pub const fn bytes(self, _children: &[&[u8]]) -> [u8; Self::LEN] {
        [
            Self::LEN as u8,
            Self::DESCRIPTOR_TYPE,
            DEVICE_CAPABILITY_TYPE_PLATFORM,
            0, // reserved
            0xdf, // platform capability UUID: Microsoft OS 2.0
            0x60,
            0xdd,
            0xd8,
            0x89,
            0x45,
            0xc7,
            0x4c,
            0x9c,
            0xd2,
            0x65,
            0x9d,
            0x9e,
            0x64,
            0x8a,
            0x9f,
            self.windows_version.to_le_bytes()[0],
            self.windows_version.to_le_bytes()[1],
            self.windows_version.to_le_bytes()[2],
            self.windows_version.to_le_bytes()[3],
            (self.msos_descriptor_len as u16).to_le_bytes()[0],
            (self.msos_descriptor_len as u16).to_le_bytes()[1],
            self.vendor_code,
            self.alt_enum_code,
        ]
    }
}

const MS_OS_20_SET_HEADER_DESCRIPTOR: u8 = 0x00;
const MS_OS_20_SUBSET_HEADER_CONFIGURATION: u8 = 0x01;
const MS_OS_20_SUBSET_HEADER_FUNCTION: u8 = 0x02;
const MS_OS_20_FEATURE_COMPATBLE_ID: u8 = 0x03;
const MS_OS_20_FEATURE_REG_PROPERTY: u8 = 0x04;
const MS_OS_20_FEATURE_CCGP_DEVICE: u8 = 0x07;

pub struct MicrosoftOs {
    pub windows_version: u32,
}

impl MicrosoftOs {
    pub const LEN: usize = 10;
    pub const DESCRIPTOR_TYPE: u8 = MS_OS_20_SET_HEADER_DESCRIPTOR;

    pub const fn bytes(self, children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut total_len = Self::LEN as u16;

        let mut i = 0;
        while i < children.len() {
            total_len += children[i].len() as u16;
            i += 1;
        }

        [
            Self::LEN as u8,
            0,
            Self::DESCRIPTOR_TYPE,
            0,
            self.windows_version.to_le_bytes()[0],
            self.windows_version.to_le_bytes()[1],
            self.windows_version.to_le_bytes()[2],
            self.windows_version.to_le_bytes()[3],
            total_len.to_le_bytes()[0],
            total_len.to_le_bytes()[1],
        ]
    }
}

pub struct MicrosoftOsConfiguration {
    /// Despite the name, it doesn't correspond directly to the configuration descriptor and is 0-indexed
    pub configuration_value: u8,
}

impl MicrosoftOsConfiguration {
    pub const LEN: usize = 8;
    pub const DESCRIPTOR_TYPE: u8 = MS_OS_20_SUBSET_HEADER_CONFIGURATION;

    pub const fn bytes(self, children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut total_len = Self::LEN as u16;

        let mut i = 0;
        while i < children.len() {
            total_len += children[i].len() as u16;
            i += 1;
        }

        [
            Self::LEN as u8,
            0,
            Self::DESCRIPTOR_TYPE,
            0,
            self.configuration_value,
            0, // reserved
            total_len.to_le_bytes()[0],
            total_len.to_le_bytes()[1],
        ]
    }
}

pub struct MicrosoftOsFunction {
    pub first_interface: u8,
}

impl MicrosoftOsFunction {
    pub const LEN: usize = 8;
    pub const DESCRIPTOR_TYPE: u8 = MS_OS_20_SUBSET_HEADER_FUNCTION;

    pub const fn bytes(self, children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut total_len = Self::LEN as u16;

        let mut i = 0;
        while i < children.len() {
            total_len += children[i].len() as u16;
            i += 1;
        }

        [
            Self::LEN as u8,
            0,
            Self::DESCRIPTOR_TYPE,
            0,
            self.first_interface,
            0, // reserved
            total_len.to_le_bytes()[0],
            total_len.to_le_bytes()[1],
        ]
    }
}

pub struct MicrosoftOsCompatibleID {
    pub compatible_id: &'static str,
    pub sub_compatible_id: &'static str,
}

impl MicrosoftOsCompatibleID {
    pub const LEN: usize = 20;
    pub const DESCRIPTOR_TYPE: u8 = MS_OS_20_FEATURE_COMPATBLE_ID;

    pub const fn bytes(self, _children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut bytes = [0; Self::LEN];
        bytes[0] = Self::LEN as u8;
        bytes[2] = Self::DESCRIPTOR_TYPE;

        let src = self.compatible_id.as_bytes();
        assert!(src.len() < 8);
        let dst = bytes.split_at_mut(4).1.split_at_mut(src.len()).0;
        dst.copy_from_slice(src);

        let src = self.sub_compatible_id.as_bytes();
        assert!(src.len() < 8);
        let dst = bytes.split_at_mut(12).1.split_at_mut(src.len()).0;
        dst.copy_from_slice(src);

        bytes
    }
}

pub struct MicrosoftOsDeviceInterfaceGUID {
    pub guid: &'static str,
}

impl MicrosoftOsDeviceInterfaceGUID {
    const PROPERTY_NAME: &'static str = "DeviceInterfaceGUIDs";
    const VALUE_LEN: usize = 38;
    pub const LEN: usize = 10 + (Self::PROPERTY_NAME.len() + 1) * 2 + (Self::VALUE_LEN + 1) * 2 + 2;
    pub const DESCRIPTOR_TYPE: u8 = MS_OS_20_FEATURE_REG_PROPERTY;

    pub const fn bytes(self, _children: &[&[u8]]) -> [u8; Self::LEN] {
        let mut bytes = [0; Self::LEN];
        bytes[0] = Self::LEN as u8;
        bytes[2] = Self::DESCRIPTOR_TYPE;
        bytes[4] = 7; // wPropertyDataType = REG_MULTI_SZ
        bytes[6] = ((Self::PROPERTY_NAME.len() + 1) * 2) as u8;

        let value_offset = 8 + (Self::PROPERTY_NAME.len() + 1) * 2;
        bytes[value_offset] = ((Self::VALUE_LEN + 1) * 2 + 2) as u8;

        let src = Self::PROPERTY_NAME.as_bytes();
        let mut i = 0;
        while i < src.len() {
            bytes[8 + i * 2] = src[i];
            i += 1;
        }

        assert!(self.guid.len() == Self::VALUE_LEN);
        let src = self.guid.as_bytes();
        let mut i = 0;
        while i < src.len() {
            bytes[value_offset + 2 + i * 2] = src[i];
            i += 1;
        }

        bytes
    }
}

pub struct MicrosoftOsCcgp {}

impl MicrosoftOsCcgp {
    pub const LEN: usize = 4;
    pub const DESCRIPTOR_TYPE: u8 = MS_OS_20_FEATURE_CCGP_DEVICE;

    pub const fn bytes(self, _children: &[&[u8]]) -> [u8; Self::LEN] {
        [
            Self::LEN as u8,
            0,
            Self::DESCRIPTOR_TYPE,
            0,
        ]
    }
}



/// Builder for runtime-generated string descriptors
pub struct DescriptorBuilder {
    buf: [u8; 80],
}

impl DescriptorBuilder {
    pub fn string<'a>(&'a mut self, s: &str) -> &'a [u8] {
        let mut len = 2;
        for c in s.encode_utf16() {
            self.buf[len..len + 2].copy_from_slice(&c.to_le_bytes());
            len += 2;
        }
        self.buf[0] = len as u8;
        self.buf[1] = usb::descriptor_type::STRING;
        &self.buf[0..len]
    }

    pub fn string_ascii<'a>(&'a mut self, s: &str) -> &'a [u8] {
        assert!(self.buf.len() >= 2 + s.len() * 2);
        let mut len = 2;
        for c in s.bytes() {
            self.buf[len] = c;
            len += 2;
        }
        self.buf[0] = len as u8;
        self.buf[1] = usb::descriptor_type::STRING;
        &self.buf[0..len]
    }

    pub fn string_hex<'a>(&'a mut self, s: &[u8]) -> &'a [u8] {
        assert!(self.buf.len() >= 2 + s.len() * 4);
        let mut len = 2;

        fn hex(b: u8) -> u8 {
            b"0123456789ABCDEF"[b as usize]
        }

        for b in s.iter() {
            self.buf[len] = hex(b >> 4);
            self.buf[len + 2] = hex(b & 0x0f);
            len += 4;
        }
        self.buf[0] = len as u8;
        self.buf[1] = usb::descriptor_type::STRING;
        &self.buf[0..len]
    }

    pub(crate) fn new() -> Self {
        Self { buf: [0; 80] }
    }
}

pub const LANGUAGE_LIST_US_ENGLISH: &[u8] = &[
    4,
    usb::descriptor_type::STRING,
    usb::language_id::ENGLISH_US.to_le_bytes()[0],
    usb::language_id::ENGLISH_US.to_le_bytes()[1],
];
