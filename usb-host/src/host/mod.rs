mod common;
#[cfg(feature = "libusb")]
mod libusb;
mod xhci;

pub use common::*;

define_int_type!(PortId, usize);

pub mod endpoint {
    pub mod kind {
        pub struct Bulk;

        impl Sealed for Bulk {}

        pub struct Isochronous;

        impl Sealed for Isochronous {}
        pub struct Interrupt;

        impl Sealed for Interrupt {}

        pub trait Sealed: Send + 'static {}
    }

    pub mod direction {
        pub struct In;

        impl Sealed for In {}
        pub struct Out;

        impl Sealed for Out {}
        pub trait Sealed: Send + 'static {}
    }
}
