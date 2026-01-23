#![cfg_attr(target_os = "none", no_std)]
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

#[macro_use]
mod _macros;

pub(crate) mod backend;
pub mod device;
pub mod err;
mod host;
pub(crate) mod hub;
mod kcore;
pub(crate) mod queue;

pub use backend::ty::Event;
pub use host::*;
pub use usb_if::{
    DrMode, Speed,
    transfer::{Direction, Recipient, Request, RequestType},
};

#[macro_use]
mod osal;
pub use osal::*;

// pub use trait_ffi::impl_extern_trait;

define_int_type!(BusAddr, u64);

pub type Mmio = NonNull<u8>;
