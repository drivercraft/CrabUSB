#![no_std]
#![feature(iterator_try_collect)]

extern crate alloc;

use core::time::Duration;

pub mod err;
mod host;
pub mod standard;
pub(crate) mod wait;

pub use futures::future::LocalBoxFuture;
pub use host::*;

pub trait Kernel {
    fn sleep<'a>(duration: Duration) -> LocalBoxFuture<'a, ()>;
    fn page_size() -> usize;
}

pub(crate) async fn sleep(duration: Duration) {
    unsafe extern "Rust" {
        fn _usb_host_sleep<'a>(duration: Duration) -> LocalBoxFuture<'a, ()>;
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
        unsafe fn _usb_host_sleep<'a>(
            duration: core::time::Duration,
        ) -> $crate::LocalBoxFuture<'a, ()> {
            <$t as $crate::Kernel>::sleep(duration)
        }

        #[unsafe(no_mangle)]
        unsafe fn _usb_host_page_size() -> usize {
            <$t as $crate::Kernel>::page_size()
        }
    };
}
