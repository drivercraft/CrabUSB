// #[cfg(feature = "libusb")]
// pub mod libusb;
pub mod xhci;

pub(crate) mod ty;

define_int_type!(Dci, u8);

impl Dci {
    pub const CTRL: Self = Self(1);

    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}
