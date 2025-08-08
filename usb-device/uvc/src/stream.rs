use crab_usb::{EndpointIsoIn, Interface};
use log::{debug, trace};

use crate::{
    VideoFormat,
    frame::{FrameEvent, FrameParser},
};

pub struct VideoStream {
    ep: EndpointIsoIn,
    _iface: Interface,
    frame_parser: FrameParser,
    pub vedio_format: VideoFormat,
}

unsafe impl Send for VideoStream {}

impl VideoStream {
    pub fn new(ep: EndpointIsoIn, iface: Interface, vfmt: VideoFormat) -> Self {
        VideoStream {
            ep,
            _iface: iface,
            frame_parser: FrameParser::new(),
            vedio_format: vfmt,
        }
    }

    pub async fn recv(&mut self) -> Result<Vec<FrameEvent>, usb_if::host::USBError> {
        let max_packet_size = self.ep.descriptor.max_packet_size;

        let mut buffer = vec![0u8; 4096]; // Adjust size as needed
        // 使用合理的 ISO packet 数量，例如根据缓冲区大小和最大包大小计算
        let num_iso_packets = if max_packet_size > 0 {
            (buffer.len() / max_packet_size as usize).clamp(1, 32)
        } else {
            8 // 默认使用 8 个包
        };
        trace!("Using {num_iso_packets} ISO packets, max_packet_size: {max_packet_size}");

        self.ep.submit(&mut buffer, num_iso_packets)?.await?;

        let mut events = Vec::new();

        for data in buffer.chunks(max_packet_size as usize) {
            if let Some(one) = self
                .frame_parser
                .push_packet(data)
                .map_err(|e| usb_if::host::USBError::Other(format!("Frame parsing error: {e:?}")))?
            {
                events.push(one);
            }
        }

        Ok(events)
    }
}
