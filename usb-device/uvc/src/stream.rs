use crab_usb::{EndpointIsoIn, Interface};
use log::debug;

use crate::VideoFrame;

pub struct VideoStream {
    ep: EndpointIsoIn,
    _iface: Interface,
}

unsafe impl Send for VideoStream {}

impl VideoStream {
    pub fn new(ep: EndpointIsoIn, iface: Interface) -> Self {
        VideoStream { ep, _iface: iface }
    }

    pub async fn recv(&mut self) -> Result<(), usb_if::host::USBError> {
        let max_packet_size = self.ep.descriptor.max_packet_size;

        let mut buffer = vec![0u8; max_packet_size as usize]; // Adjust size as needed
        let transfer = self.ep.submit(&mut buffer, 1)?.await?;
        // debug!("Received video frame of size: {transfer}");

        Ok(())
    }
}
