/// USB Device Class Codes as defined by USB-IF
/// https://www.usb.org/defined-class-codes
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassCode(pub u8);

impl ClassCode {
    /// Use class information in the Interface Descriptors
    pub const USE_INTERFACE: ClassCode = ClassCode(0x00);
    /// Audio device
    pub const AUDIO: ClassCode = ClassCode(0x01);
    /// Communications and CDC Control
    pub const COMMUNICATION: ClassCode = ClassCode(0x02);
    /// HID (Human Interface Device)
    pub const HID: ClassCode = ClassCode(0x03);
    /// Physical device
    pub const PHYSICAL: ClassCode = ClassCode(0x05);
    /// Still Imaging device
    pub const STILL_IMAGING: ClassCode = ClassCode(0x06);
    /// Printer device
    pub const PRINTER: ClassCode = ClassCode(0x07);
    /// Mass Storage device
    pub const MASS_STORAGE: ClassCode = ClassCode(0x08);
    /// Hub device
    pub const HUB: ClassCode = ClassCode(0x09);
    /// CDC-Data
    pub const CDC_DATA: ClassCode = ClassCode(0x0A);
    /// Smart Card device
    pub const SMART_CARD: ClassCode = ClassCode(0x0B);
    /// Content Security device
    pub const CONTENT_SECURITY: ClassCode = ClassCode(0x0D);
    /// Video device
    pub const VIDEO: ClassCode = ClassCode(0x0E);
    /// Personal Healthcare device
    pub const PERSONAL_HEALTHCARE: ClassCode = ClassCode(0x0F);
    /// Audio/Video Devices
    pub const AUDIO_VIDEO: ClassCode = ClassCode(0x10);
    /// Billboard Device Class
    pub const BILLBOARD: ClassCode = ClassCode(0x11);
    /// USB Type-C Bridge Class
    pub const TYPE_C_BRIDGE: ClassCode = ClassCode(0x12);
    /// USB Bulk Display Protocol Device Class
    pub const BULK_DISPLAY_PROTOCOL: ClassCode = ClassCode(0x13);
    /// MCTP over USB Protocol Endpoint Device Class
    pub const MCTP_OVER_USB: ClassCode = ClassCode(0x14);
    /// I3C Device Class
    pub const I3C: ClassCode = ClassCode(0x3C);
    /// Diagnostic Device
    pub const DIAGNOSTIC: ClassCode = ClassCode(0xDC);
    /// Wireless Controller
    pub const WIRELESS: ClassCode = ClassCode(0xE0);
    /// Miscellaneous
    pub const MISCELLANEOUS: ClassCode = ClassCode(0xEF);
    /// Application Specific
    pub const APPLICATION: ClassCode = ClassCode(0xFE);
    /// Vendor Specific
    pub const VENDOR: ClassCode = ClassCode(0xFF);
}

impl core::fmt::Debug for ClassCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple(match self.0 {
            0x00 => "USE_INTERFACE",
            0x01 => "AUDIO",
            0x02 => "COMMUNICATION",
            0x03 => "HID",
            0x05 => "PHYSICAL",
            0x06 => "STILL_IMAGING",
            0x07 => "PRINTER",
            0x08 => "MASS_STORAGE",
            0x09 => "HUB",
            0x0A => "CDC_DATA",
            0x0B => "SMART_CARD",
            0x0D => "CONTENT_SECURITY",
            0x0E => "VIDEO",
            0x0F => "PERSONAL_HEALTHCARE",
            0x10 => "AUDIO_VIDEO",
            0x11 => "BILLBOARD",
            0x12 => "TYPE_C_BRIDGE",
            0x13 => "BULK_DISPLAY_PROTOCOL",
            0x14 => "MCTP_OVER_USB",
            0x3C => "I3C",
            0xDC => "DIAGNOSTIC",
            0xE0 => "WIRELESS",
            0xEF => "MISCELLANEOUS",
            0xFE => "APPLICATION",
            0xFF => "VENDOR",
            _ => "UNKNOWN",
        })
        .field(&self.0)
        .finish()
    }
}

impl From<u8> for ClassCode {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<ClassCode> for u8 {
    fn from(class_code: ClassCode) -> Self {
        class_code.0
    }
}
