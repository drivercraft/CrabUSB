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

fn parse_default_max_packet_size_from_port_speed(speed: u8) -> u16 {
    match speed {
        1 => 8,
        2 | 3 => 64,
        4..=6 => 512,
        v => unimplemented!("PSI: {}", v),
    }
}
