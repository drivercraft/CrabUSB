#![cfg_attr(not(feature = "libusb"), no_std)]
#![feature(iterator_try_collect)]

extern crate alloc;
#[macro_use]
extern crate log;

use core::ptr::NonNull;

pub use usb_if::descriptor::*;
pub use usb_if::err::*;
pub use usb_if::transfer::*;

#[macro_use]
mod _macros;

pub(crate) mod backend;
pub mod err;
mod host;

pub use host::*;

#[macro_use]
mod osal;
pub use osal::Kernel;
pub use trait_ffi::impl_extern_trait;

define_int_type!(BusAddr, u64);

pub type Mmio = NonNull<u8>;
