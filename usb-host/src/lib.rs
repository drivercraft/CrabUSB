#![cfg_attr(not(feature = "libusb"), no_std)]
#![feature(iterator_try_collect)]

extern crate alloc;

use core::time::Duration;
pub use usb_if::descriptor::*;
pub use usb_if::err::*;
pub use usb_if::transfer::*;

#[macro_use]
mod _macros;

pub mod err;
mod host;

pub use futures::future::BoxFuture;
pub use host::*;

pub trait Kernel {
    fn sleep<'a>(duration: Duration) -> BoxFuture<'a, ()>;
    fn page_size() -> usize;
}

pub(crate) async fn sleep(duration: Duration) {
    unsafe extern "Rust" {
        fn _usb_host_sleep<'a>(duration: Duration) -> BoxFuture<'a, ()>;
    }

    unsafe {
        _usb_host_sleep(duration).await;
    }
}

pub(crate) fn page_size() -> usize {
    unsafe {
        unsafe extern "Rust" {
            fn _usb_host_page_size() -> usize;
        }

        _usb_host_page_size()
    }
}

#[macro_export]
macro_rules! set_impl {
    ($t: ty) => {
        #[unsafe(no_mangle)]
        unsafe fn _usb_host_sleep<'a>(duration: core::time::Duration) -> $crate::BoxFuture<'a, ()> {
            <$t as $crate::Kernel>::sleep(duration)
        }

        #[unsafe(no_mangle)]
        unsafe fn _usb_host_page_size() -> usize {
            <$t as $crate::Kernel>::page_size()
        }
    };
}

define_int_type!(BusAddr, u64);
