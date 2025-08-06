use num_enum::{FromPrimitive, IntoPrimitive};

/// USB Device Class Codes as defined by USB-IF
/// https://www.usb.org/defined-class-codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, FromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum BaseClass {
    /// Use class information in the Interface Descriptors
    UseInterface = 0x00,
    /// Audio device
    Audio = 0x01,
    /// Communications and CDC Control
    Communication = 0x02,
    /// HID (Human Interface Device)
    Hid = 0x03,
    /// Physical device
    Physical = 0x05,
    /// Still Imaging device
    StillImaging = 0x06,
    /// Printer device
    Printer = 0x07,
    /// Mass Storage device
    MassStorage = 0x08,
    /// Hub device
    Hub = 0x09,
    /// CDC-Data
    CdcData = 0x0A,
    /// Smart Card device
    SmartCard = 0x0B,
    /// Content Security device
    ContentSecurity = 0x0D,
    /// Video device
    Video = 0x0E,
    /// Personal Healthcare device
    PersonalHealthcare = 0x0F,
    /// Audio/Video Devices
    AudioVideo = 0x10,
    /// Billboard Device Class
    Billboard = 0x11,
    /// USB Type-C Bridge Class
    TypeCBridge = 0x12,
    /// USB Bulk Display Protocol Device Class
    BulkDisplayProtocol = 0x13,
    /// MCTP over USB Protocol Endpoint Device Class
    MctpOverUsb = 0x14,
    /// I3C Device Class
    I3c = 0x3C,
    /// Diagnostic Device
    Diagnostic = 0xDC,
    /// Wireless Controller
    Wireless = 0xE0,
    /// Miscellaneous
    Miscellaneous = 0xEF,
    /// Other/Unknown class codes
    #[num_enum(catch_all)]
    Other(u8),
    /// Application Specific
    Application = 0xFE,
    /// Vendor Specific
    Vendor = 0xFF,
}

pub struct SubClass(pub u8);
