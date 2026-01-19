pub(crate) mod cmd;
mod context;
mod def;
pub(crate) mod device;
mod endpoint;
mod event;
pub(crate) mod host;
pub(crate) mod hub;
pub(crate) mod port;
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

fn append_port_to_route_string(route_string: u32, port_id: usize) -> u32 {
    let mut route_string = route_string;
    for tier in 0..5 {
        if route_string & (0x0f << (tier * 4)) == 0 && tier < 5 {
            route_string |= (port_id as u32) << (tier * 4);
            return route_string;
        }
    }

    route_string
}
