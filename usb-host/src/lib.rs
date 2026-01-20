#![cfg_attr(not(any(windows, unix)), no_std)]
#![feature(iterator_try_collect)]

#[macro_use]
extern crate alloc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate anyhow;

use core::ptr::NonNull;

pub use usb_if::descriptor::*;
pub use usb_if::err::*;
pub use usb_if::transfer::*;

#[macro_use]
mod _macros;

pub(crate) mod backend;
pub mod device;
pub mod err;
mod host;
pub(crate) mod hub;
pub(crate) mod queue;

pub use backend::ty::Event;
pub use host::*;
pub use usb_if::{DeviceSpeed, DrMode};

#[macro_use]
mod osal;
pub use osal::Kernel;
pub use trait_ffi::impl_extern_trait;

define_int_type!(BusAddr, u64);

pub type Mmio = NonNull<u8>;
