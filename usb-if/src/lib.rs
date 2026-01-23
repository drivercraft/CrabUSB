#![no_std]

extern crate alloc;

pub mod descriptor;
pub mod err;
pub mod host;
pub mod transfer;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrMode {
    #[default]
    Host,
    Peripheral,
    Otg,
}

// 重新导出 host::hub::DeviceSpeed，避免重复定义
pub use host::hub::DeviceSpeed;
