pub(crate) mod cmd;
mod context;
mod def;
pub(crate) mod device;
mod endpoint;
mod event;
pub(crate) mod host;
pub(crate) mod hub;
mod reg;
mod ring;
mod sync;
mod transfer;

pub(crate) use def::*;

pub use device::Device;
pub use host::Xhci;

use usb_if::host::hub::DeviceSpeed;

fn parse_default_max_packet_size_from_port_speed(speed: DeviceSpeed) -> u16 {
    // 根据 xHCI 规范表 6-30 和 U-Boot 实现：
    // 参考 U-Boot drivers/usb/host/xhci-mem.c:730-751
    match speed {
        DeviceSpeed::Full => 64,             // Full Speed → 64 bytes
        DeviceSpeed::Low => 8,               // Low Speed → 8 bytes
        DeviceSpeed::High => 64,             // High Speed → 64 bytes
        DeviceSpeed::SuperSpeed => 512,      // SuperSpeed → 512 bytes
        DeviceSpeed::SuperSpeedPlus => 1024, // SuperSpeedPlus → 1024 bytes
        DeviceSpeed::Wireless => unimplemented!("Wireless"),
    }
}
