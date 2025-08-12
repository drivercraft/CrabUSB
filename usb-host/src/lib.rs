#![cfg_attr(not(feature = "libusb"), no_std)]
#![feature(iterator_try_collect)]

extern crate alloc;

pub use usb_if::descriptor::*;
pub use usb_if::err::*;
pub use usb_if::transfer::*;

#[macro_use]
mod _macros;

pub mod err;
pub mod host;

pub use futures::future::BoxFuture;
pub use host::*;

#[macro_use]
mod osal;
pub use osal::Kernel;
pub use trait_ffi::impl_extern_trait;

define_int_type!(BusAddr, u64);
