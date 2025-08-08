use crate::descriptors::payload_header_flags as flags;
use crab_usb::TransferError;
use log::{debug, trace};

/// UVC 载荷头（2.4.3.3）
#[derive(Debug, Clone, Default)]
pub struct UvcPayloadHeader {
    pub length: u8,              // bLength
    pub info: u8,                // bmHeaderInfo
    pub fid: bool,               // Frame ID
    pub eof: bool,               // End of Frame
    pub pts: Option<u32>,        // Presentation Time Stamp (4 bytes, 90kHz)
    pub scr: Option<(u32, u16)>, // Source Clock Reference: SOF timestamp (32) + SOF count (16)
    pub has_err: bool,
}

impl UvcPayloadHeader {
    /// 从字节流解析 UVC 载荷头；若数据不合法，返回 None 以允许上层丢弃该包。
    pub fn parse(buf: &[u8]) -> Option<(Self, usize)> {
        if buf.len() < 2 {
            return None;
        }
        let b_length = buf[0] as usize;
        let info = buf[1];
        if b_length < 2 || b_length > buf.len() {
            return None;
        }

        let fid = (info & flags::FID) != 0;
        let eof = (info & flags::EOF) != 0;
        let has_pts = (info & flags::PTS) != 0;
        let has_scr = (info & flags::SCR) != 0;
        let has_err = (info & flags::ERR) != 0;

        // 可选字段顺序：PTS(4) -> SCR(6)
        let mut offset = 2usize;
        let pts = if has_pts {
            if offset + 4 > b_length {
                return None;
            }
            let v = u32::from_le_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            offset += 4;
            Some(v)
        } else {
            None
        };

        let scr = if has_scr {
            if offset + 6 > b_length {
                return None;
            }
            let stc = u32::from_le_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            let sof = u16::from_le_bytes([buf[offset + 4], buf[offset + 5]]);
            offset += 6;
            Some((stc, sof))
        } else {
            None
        };

        // 剩余可忽略的扩展字段由 b_length 统一跳过
        let header = UvcPayloadHeader {
            length: b_length as u8,
            info,
            fid,
            eof,
            pts,
            scr,
            has_err,
        };

        Some((header, b_length))
    }
}

/// 帧组装事件（供上层转换为具体视频帧结构）
#[derive(Debug, Clone)]
pub struct FrameEvent {
    pub data: Vec<u8>,
    pub pts_90khz: Option<u32>,
    pub eof: bool,
    pub fid: bool,
    pub frame_number: u32,
}

/// UVC 帧解析/组装器（参考 libuvc 的 FID 翻转与 EOF 逻辑）
#[derive(Debug)]
pub struct FrameParser {
    buffer: Vec<u8>,
    last_fid: Option<bool>,
    frame_number: u32,
    last_pts: Option<u32>,
}

impl Default for FrameParser {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameParser {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(1 << 20),
            last_fid: None,
            frame_number: 0,
            last_pts: None,
        }
    }

    /// 处理一包 UVC 传输数据；返回完整帧事件（若 EOF 收到）
    pub fn push_packet(&mut self, data: &[u8]) -> Result<Option<FrameEvent>, TransferError> {
        if data.len() < 2 {
            return Ok(None);
        }

        let (hdr, hdr_len) = match UvcPayloadHeader::parse(data) {
            Some(v) => v,
            None => {
                debug!(
                    "Invalid UVC payload header, dropping packet: {} bytes",
                    data.len()
                );
                return Ok(None);
            }
        };

        if hdr.has_err {
            trace!(
                "UVC payload ERR set; dropping current buffer ({} bytes)",
                self.buffer.len()
            );
            self.buffer.clear();
            self.last_pts = None;
            // 继续后面的包
            return Ok(None);
        }

        // FID 翻转表示新帧开始；若上一个未完成，直接丢弃以对齐
        if let Some(last) = self.last_fid
            && last != hdr.fid
            && !self.buffer.is_empty()
        {
            trace!(
                "FID toggled ({} -> {}), discard {} bytes of incomplete frame",
                last,
                hdr.fid,
                self.buffer.len()
            );
            self.buffer.clear();
        }
        self.last_fid = Some(hdr.fid);

        // 载荷数据在头之后
        if hdr_len <= data.len() {
            let payload = &data[hdr_len..];
            if !payload.is_empty() {
                self.buffer.extend_from_slice(payload);
            }
        }
        if let Some(pts) = hdr.pts {
            self.last_pts = Some(pts);
        }

        if hdr.eof {
            if self.buffer.is_empty() {
                // 某些设备会发送空 EOF 包，忽略
                return Ok(None);
            }
            let mut out = Vec::with_capacity(self.buffer.len());
            out.extend_from_slice(&self.buffer);
            self.buffer.clear();

            let evt = FrameEvent {
                data: out,
                pts_90khz: self.last_pts.take(),
                eof: true,
                fid: hdr.fid,
                frame_number: self.frame_number,
            };
            self.frame_number = self.frame_number.wrapping_add(1);
            return Ok(Some(evt));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 构造一个包含两包的简单帧：第一包无 EOF，第二包 EOF
    #[test]
    fn assemble_simple_frame() {
        let mut p = FrameParser::new();

        // 包1：bLength=6 (2 固定 + 4 PTS), bmHeaderInfo=FID|PTS(无EOF)，负载= [1,2,3]
        let mut pkt1 = vec![6u8, flags::FID | flags::PTS];
        // PTS 4 字节
        pkt1.extend_from_slice(&0x11223344u32.to_le_bytes());
        pkt1.extend_from_slice(&[1, 2, 3]);

        // 包2：bLength=2, bmHeaderInfo=FID|EOF
        let mut pkt2 = vec![2u8, flags::FID | flags::EOF];
        pkt2.extend_from_slice(&[4, 5]);

        assert!(matches!(p.push_packet(&pkt1), Ok(None)));
        let ev = p
            .push_packet(&pkt2)
            .unwrap()
            .expect("frame should complete");
        assert_eq!(ev.data, vec![1, 2, 3, 4, 5]);
        assert!(ev.eof);
        assert_eq!(ev.frame_number, 0);
    }
}
