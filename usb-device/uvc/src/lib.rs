use crab_usb::{
    Class, Device, DeviceInfo, Direction, EndpointIsoIn, EndpointType, Interface, TransferError,
    err::USBError,
};
use log::debug;

/// UVC 视频格式类型
#[derive(Debug, Clone, PartialEq)]
pub enum VideoFormat {
    /// 未压缩格式 (如 YUV)
    Uncompressed {
        width: u16,
        height: u16,
        frame_rate: u32, // 帧率 (fps)
        format_type: UncompressedFormat,
    },
    /// MJPEG 压缩格式
    Mjpeg {
        width: u16,
        height: u16,
        frame_rate: u32,
    },
    /// H.264 压缩格式
    H264 {
        width: u16,
        height: u16,
        frame_rate: u32,
    },
}

/// 未压缩视频格式类型
#[derive(Debug, Clone, PartialEq)]
pub enum UncompressedFormat {
    /// YUY2 (YUYV) 格式
    Yuy2,
    /// NV12 格式
    Nv12,
    /// RGB24 格式
    Rgb24,
    /// RGB32 格式
    Rgb32,
}

/// 视频控制事件
#[derive(Debug, Clone)]
pub enum VideoControlEvent {
    /// 视频格式变更
    FormatChanged(VideoFormat),
    /// 亮度调整
    BrightnessChanged(i16),
    /// 对比度调整
    ContrastChanged(i16),
    /// 色调调整
    HueChanged(i16),
    /// 饱和度调整
    SaturationChanged(i16),
    /// 错误事件
    Error(String),
}

/// 视频数据帧
#[derive(Debug)]
pub struct VideoFrame {
    /// 帧数据
    pub data: Vec<u8>,
    /// 时间戳
    pub timestamp: u64,
    /// 帧序号
    pub frame_number: u32,
    /// 数据格式
    pub format: VideoFormat,
    /// 是否是帧结束标志
    pub end_of_frame: bool,
}

/// UVC 设备状态
#[derive(Debug, Clone, PartialEq)]
pub enum UvcDeviceState {
    /// 未配置
    Unconfigured,
    /// 已配置但未开始流传输
    Configured,
    /// 正在进行流传输
    Streaming,
    /// 错误状态
    Error(String),
}

pub struct UvcDevice {
    _device: Device,
    video_control_interface: Interface,
    video_streaming_interface: Option<Interface>,
    ep_in: Option<EndpointIsoIn>,
    current_format: Option<VideoFormat>,
    state: UvcDeviceState,
    frame_buffer: Vec<u8>,
    current_frame_number: u32,
}

impl UvcDevice {
    /// 检查设备是否为 UVC 设备
    pub fn check(info: &DeviceInfo) -> bool {
        let mut has_video_control = false;
        let mut has_video_streaming = false;

        for iface in info.interface_descriptors() {
            match iface.class() {
                Class::Video | Class::AudioVideo(_) => {
                    // UVC Video Control Interface (subclass=1)
                    if iface.subclass == 1 {
                        has_video_control = true;
                    }
                    // UVC Video Streaming Interface (subclass=2)
                    if iface.subclass == 2 {
                        has_video_streaming = true;
                    }
                }
                _ => {}
            }
        }

        has_video_control && has_video_streaming
    }

    /// 创建新的 UVC 设备实例
    pub async fn new(mut device: Device) -> Result<Self, USBError> {
        for config in device.configurations.iter() {
            debug!("Configuration: {config:?}");
        }

        // 首先保存需要的接口信息，避免同时持有可变和不可变引用
        let (video_control_info, video_streaming_info) = {
            let config = &device.configurations[0];

            // 查找 Video Control Interface (class=14, subclass=1)
            let video_control_iface = config
                .interfaces
                .iter()
                .find(|iface| {
                    let iface = iface.first_alt_setting();
                    matches!(iface.class(), Class::Video) && iface.subclass == 1
                })
                .ok_or(USBError::NotFound)?
                .first_alt_setting();

            // 查找 Video Streaming Interface (class=14, subclass=2)
            let video_streaming_iface = config
                .interfaces
                .iter()
                .find(|iface| {
                    let iface = iface.first_alt_setting();
                    matches!(iface.class(), Class::Video) && iface.subclass == 2
                })
                .map(|iface| iface.first_alt_setting());

            (
                (
                    video_control_iface.interface_number,
                    video_control_iface.alternate_setting,
                ),
                video_streaming_iface.map(|vs| (vs.interface_number, vs.alternate_setting)),
            )
        };

        debug!("Using Video Control interface: {:?}", video_control_info);

        let video_control_interface = device
            .claim_interface(video_control_info.0, video_control_info.1)
            .await?;

        let mut video_streaming_interface = None;
        let mut ep_in = None;

        if let Some((vs_interface_num, vs_alt_setting)) = video_streaming_info {
            debug!(
                "Using Video Streaming interface: {} alt {}",
                vs_interface_num, vs_alt_setting
            );

            let mut vs_interface = device
                .claim_interface(vs_interface_num, vs_alt_setting)
                .await?;

            // 查找同步 IN 端点用于视频数据传输
            for endpoint in vs_interface.descriptor.endpoints.clone().into_iter() {
                match (endpoint.transfer_type, endpoint.direction) {
                    (EndpointType::Isochronous, Direction::In) => {
                        debug!("Found isochronous IN endpoint: {endpoint:?}");
                        ep_in = Some(vs_interface.endpoint_iso_in(endpoint.address)?);
                        break;
                    }
                    _ => {
                        debug!("Ignoring endpoint: {endpoint:?}");
                    }
                }
            }

            video_streaming_interface = Some(vs_interface);
        }

        Ok(Self {
            _device: device,
            video_control_interface,
            video_streaming_interface,
            ep_in,
            current_format: None,
            state: UvcDeviceState::Configured,
            frame_buffer: Vec::new(),
            current_frame_number: 0,
        })
    }

    /// 获取设备支持的视频格式列表
    pub async fn get_supported_formats(&self) -> Result<Vec<VideoFormat>, USBError> {
        // 在实际实现中，这里应该解析 UVC 格式描述符
        // 为了简化，这里返回一些常见格式
        let formats = vec![
            VideoFormat::Mjpeg {
                width: 640,
                height: 480,
                frame_rate: 30,
            },
            VideoFormat::Mjpeg {
                width: 1280,
                height: 720,
                frame_rate: 30,
            },
            VideoFormat::Uncompressed {
                width: 640,
                height: 480,
                frame_rate: 30,
                format_type: UncompressedFormat::Yuy2,
            },
        ];

        Ok(formats)
    }

    /// 设置视频格式
    pub async fn set_format(&mut self, format: VideoFormat) -> Result<(), USBError> {
        // 在实际实现中，这里应该向设备发送 SET_CUR 控制请求
        // 设置 VS_COMMIT_CONTROL 来配置视频格式
        debug!("Setting video format: {:?}", format);

        self.current_format = Some(format);
        Ok(())
    }

    /// 开始视频流传输
    pub async fn start_streaming(&mut self) -> Result<(), USBError> {
        if self.ep_in.is_none() {
            return Err(USBError::NotFound);
        }

        if self.current_format.is_none() {
            return Err(USBError::Other("No format selected".into()));
        }

        // 在实际实现中，这里应该：
        // 1. 选择合适的 alternate setting（具有带宽的那个）
        // 2. 发送 SET_INTERFACE 请求
        debug!("Starting video streaming");

        self.state = UvcDeviceState::Streaming;
        Ok(())
    }

    /// 停止视频流传输
    pub async fn stop_streaming(&mut self) -> Result<(), USBError> {
        debug!("Stopping video streaming");

        // 选择 alternate setting 0（无带宽）
        self.state = UvcDeviceState::Configured;
        Ok(())
    }

    /// 接收视频帧数据
    pub async fn recv_frame(&mut self) -> Result<Option<VideoFrame>, TransferError> {
        if self.state != UvcDeviceState::Streaming {
            return Ok(None);
        }

        let ep_in = match &mut self.ep_in {
            Some(ep) => ep,
            None => return Ok(None),
        };

        // UVC 设备通常使用较大的缓冲区来接收视频数据
        let mut buf = vec![0u8; 1024]; // 实际大小应该基于端点的最大包大小

        match ep_in.submit(&mut buf, 1)?.await {
            Ok(n) => {
                if n == 0 {
                    return Ok(None);
                }

                let data = &buf[..n];

                // UVC 视频数据包格式分析
                if data.len() < 2 {
                    return Ok(None);
                }

                // UVC 载荷头分析 (简化版本)
                let header_length = data[0] as usize;
                if header_length > data.len() {
                    return Ok(None);
                }

                let header_info = data[1];
                let _frame_id = (header_info & 0x01) != 0;
                let end_of_frame = (header_info & 0x02) != 0;
                let presentation_time = (header_info & 0x04) != 0;
                let _source_clock_ref = (header_info & 0x08) != 0;
                let error = (header_info & 0x40) != 0;

                if error {
                    debug!("UVC payload error detected");
                    return Ok(None);
                }

                // 提取实际的视频数据（跳过载荷头）
                let payload_data = &data[header_length..];

                // 将数据添加到帧缓冲区
                self.frame_buffer.extend_from_slice(payload_data);

                if end_of_frame {
                    // 完整帧接收完成
                    let frame = VideoFrame {
                        data: self.frame_buffer.clone(),
                        timestamp: if presentation_time {
                            // 在实际实现中应该从载荷头中提取时间戳
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_micros() as u64
                        } else {
                            0
                        },
                        frame_number: self.current_frame_number,
                        format: self.current_format.clone().unwrap_or(VideoFormat::Mjpeg {
                            width: 640,
                            height: 480,
                            frame_rate: 30,
                        }),
                        end_of_frame: true,
                    };

                    self.frame_buffer.clear();
                    self.current_frame_number += 1;

                    debug!("Received complete video frame: {} bytes", frame.data.len());
                    Ok(Some(frame))
                } else {
                    // 帧数据的一部分，继续接收
                    Ok(None)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// 获取当前设备状态
    pub fn get_state(&self) -> &UvcDeviceState {
        &self.state
    }

    /// 获取当前视频格式
    pub fn get_current_format(&self) -> Option<&VideoFormat> {
        self.current_format.as_ref()
    }

    /// 发送视频控制命令
    pub async fn send_control_command(
        &mut self,
        command: VideoControlEvent,
    ) -> Result<(), USBError> {
        debug!("Sending video control command: {:?}", command);

        // 在实际实现中，这里应该构造并发送相应的 UVC 控制请求
        // 例如 SET_CUR 请求来设置亮度、对比度等参数
        match command {
            VideoControlEvent::BrightnessChanged(value) => {
                // 发送 SET_CUR(PU_BRIGHTNESS_CONTROL) 请求
                debug!("Setting brightness to: {}", value);
            }
            VideoControlEvent::ContrastChanged(value) => {
                // 发送 SET_CUR(PU_CONTRAST_CONTROL) 请求
                debug!("Setting contrast to: {}", value);
            }
            VideoControlEvent::HueChanged(value) => {
                // 发送 SET_CUR(PU_HUE_CONTROL) 请求
                debug!("Setting hue to: {}", value);
            }
            VideoControlEvent::SaturationChanged(value) => {
                // 发送 SET_CUR(PU_SATURATION_CONTROL) 请求
                debug!("Setting saturation to: {}", value);
            }
            _ => {
                debug!("Control command not implemented: {:?}", command);
            }
        }

        Ok(())
    }

    /// 获取设备信息字符串
    pub async fn get_device_info(&self) -> Result<String, USBError> {
        // 在实际实现中，这里可以读取设备的字符串描述符
        Ok("UVC Video Device".to_string())
    }
}
