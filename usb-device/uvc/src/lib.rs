use crab_usb::{
    Class, Device, DeviceInfo, Direction, EndpointIsoIn, EndpointType, Interface, Recipient,
    Request, RequestType, TransferError, err::USBError,
};
use log::debug;
use usb_if::host::ControlSetup;

// UVC常量定义
/// UVC类特定请求
pub mod uvc_requests {
    pub const SET_CUR: u8 = 0x01;
    pub const GET_CUR: u8 = 0x81;
    pub const GET_MIN: u8 = 0x82;
    pub const GET_MAX: u8 = 0x83;
    pub const GET_RES: u8 = 0x84;
    pub const GET_LEN: u8 = 0x85;
    pub const GET_INFO: u8 = 0x86;
    pub const GET_DEF: u8 = 0x87;
}

/// UVC处理单元控制选择器
pub mod pu_controls {
    pub const PU_BRIGHTNESS_CONTROL: u8 = 0x02;
    pub const PU_CONTRAST_CONTROL: u8 = 0x03;
    pub const PU_HUE_CONTROL: u8 = 0x06;
    pub const PU_SATURATION_CONTROL: u8 = 0x07;
    pub const PU_SHARPNESS_CONTROL: u8 = 0x08;
    pub const PU_GAMMA_CONTROL: u8 = 0x09;
    pub const PU_WHITE_BALANCE_TEMPERATURE_CONTROL: u8 = 0x0A;
    pub const PU_WHITE_BALANCE_COMPONENT_CONTROL: u8 = 0x0B;
    pub const PU_BACKLIGHT_COMPENSATION_CONTROL: u8 = 0x0C;
    pub const PU_GAIN_CONTROL: u8 = 0x0D;
    pub const PU_POWER_LINE_FREQUENCY_CONTROL: u8 = 0x0E;
    pub const PU_HUE_AUTO_CONTROL: u8 = 0x0F;
    pub const PU_WHITE_BALANCE_TEMPERATURE_AUTO_CONTROL: u8 = 0x10;
    pub const PU_WHITE_BALANCE_COMPONENT_AUTO_CONTROL: u8 = 0x11;
}

/// UVC视频流控制选择器
pub mod vs_controls {
    pub const VS_PROBE_CONTROL: u8 = 0x01;
    pub const VS_COMMIT_CONTROL: u8 = 0x02;
    pub const VS_STILL_PROBE_CONTROL: u8 = 0x03;
    pub const VS_STILL_COMMIT_CONTROL: u8 = 0x04;
}

/// UVC终端类型
pub mod terminal_types {
    pub const TT_VENDOR_SPECIFIC: u16 = 0x0100;
    pub const TT_STREAMING: u16 = 0x0101;
    pub const ITT_VENDOR_SPECIFIC: u16 = 0x0200;
    pub const ITT_CAMERA: u16 = 0x0201;
    pub const ITT_MEDIA_TRANSPORT_INPUT: u16 = 0x0202;
    pub const OTT_VENDOR_SPECIFIC: u16 = 0x0300;
    pub const OTT_DISPLAY: u16 = 0x0301;
    pub const OTT_MEDIA_TRANSPORT_OUTPUT: u16 = 0x0302;
}

/// UVC描述符类型
pub mod uvc_descriptor_types {
    pub const CS_INTERFACE: u8 = 0x24;
    pub const CS_ENDPOINT: u8 = 0x25;
}

/// UVC接口描述符子类型
pub mod uvc_interface_subtypes {
    pub const VC_DESCRIPTOR_UNDEFINED: u8 = 0x00;
    pub const VC_HEADER: u8 = 0x01;
    pub const VC_INPUT_TERMINAL: u8 = 0x02;
    pub const VC_OUTPUT_TERMINAL: u8 = 0x03;
    pub const VC_SELECTOR_UNIT: u8 = 0x04;
    pub const VC_PROCESSING_UNIT: u8 = 0x05;
    pub const VC_EXTENSION_UNIT: u8 = 0x06;

    pub const VS_UNDEFINED: u8 = 0x00;
    pub const VS_INPUT_HEADER: u8 = 0x01;
    pub const VS_OUTPUT_HEADER: u8 = 0x02;
    pub const VS_STILL_IMAGE_FRAME: u8 = 0x03;
    pub const VS_FORMAT_UNCOMPRESSED: u8 = 0x04;
    pub const VS_FRAME_UNCOMPRESSED: u8 = 0x05;
    pub const VS_FORMAT_MJPEG: u8 = 0x06;
    pub const VS_FRAME_MJPEG: u8 = 0x07;
    pub const VS_FORMAT_MPEG2TS: u8 = 0x0A;
    pub const VS_FORMAT_DV: u8 = 0x0C;
    pub const VS_COLORFORMAT: u8 = 0x0D;
    pub const VS_FORMAT_FRAME_BASED: u8 = 0x10;
    pub const VS_FRAME_FRAME_BASED: u8 = 0x11;
    pub const VS_FORMAT_STREAM_BASED: u8 = 0x12;
    pub const VS_FORMAT_H264: u8 = 0x13;
    pub const VS_FRAME_H264: u8 = 0x14;
    pub const VS_FORMAT_H264_SIMULCAST: u8 = 0x15;
}

/// UVC GUID常量
pub mod uvc_guids {
    // YUY2 格式 GUID
    pub const YUY2: [u8; 16] = [
        0x59, 0x55, 0x59, 0x32, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ];

    // NV12 格式 GUID
    pub const NV12: [u8; 16] = [
        0x4e, 0x56, 0x31, 0x32, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ];

    // RGB24 格式 GUID (RGB3)
    pub const RGB24: [u8; 16] = [
        0x52, 0x47, 0x42, 0x33, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ];
}

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

/// 当前正在解析的格式类型
#[derive(Debug, Clone)]
enum CurrentFormatType {
    Mjpeg,
    Uncompressed(UncompressedFormat),
    H264,
}

pub struct UvcDevice {
    device: Device,
    video_control_interface: Interface,
    video_streaming_interface: Option<Interface>,
    video_streaming_interface_num: Option<u8>,
    processing_unit_id: Option<u8>, // 处理单元ID
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

        debug!("Using Video Control interface: {video_control_info:?}");

        let video_control_interface = device
            .claim_interface(video_control_info.0, video_control_info.1)
            .await?;

        let mut video_streaming_interface = None;
        let mut ep_in = None;

        if let Some((vs_interface_num, vs_alt_setting)) = video_streaming_info {
            debug!("Using Video Streaming interface: {vs_interface_num} alt {vs_alt_setting}");

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
            device,
            video_control_interface,
            video_streaming_interface,
            video_streaming_interface_num: video_streaming_info.map(|(num, _)| num),
            processing_unit_id: Some(1), // 通常处理单元ID为1，实际应用中应该解析描述符
            ep_in,
            current_format: None,
            state: UvcDeviceState::Configured,
            frame_buffer: Vec::new(),
            current_frame_number: 0,
        })
    }

    /// 获取设备支持的视频格式列表
    pub async fn get_supported_formats(&mut self) -> Result<Vec<VideoFormat>, USBError> {
        let mut formats = Vec::new();

        // 获取完整的配置描述符来解析VS接口的额外描述符
        if let Some(vs_interface_num) = self.video_streaming_interface_num {
            debug!("Parsing VS interface {vs_interface_num} descriptors");

            // 首先尝试通过GET_DESCRIPTOR控制请求获取完整的配置描述符
            match self.get_full_configuration_descriptor().await {
                Ok(config_data) => {
                    debug!(
                        "Got full configuration descriptor: {} bytes",
                        config_data.len()
                    );

                    // 解析配置描述符中的VS接口部分
                    if let Ok(parsed_formats) =
                        self.parse_vs_interface_descriptors(&config_data, vs_interface_num)
                        && !parsed_formats.is_empty()
                    {
                        debug!(
                            "Parsed {} formats from VS interface descriptors",
                            parsed_formats.len()
                        );
                        formats.extend(parsed_formats);
                    }
                }
                Err(e) => {
                    debug!("Failed to get full configuration descriptor: {e:?}");
                }
            }

            // 如果上面的方法失败，尝试获取VS接口特定的描述符
            if formats.is_empty() {
                match self.get_vs_interface_descriptor(vs_interface_num).await {
                    Ok(vs_desc_data) => {
                        debug!("Got VS interface descriptor: {} bytes", vs_desc_data.len());
                        if let Ok(parsed_formats) = self.parse_format_descriptors(&vs_desc_data)
                            && !parsed_formats.is_empty()
                        {
                            debug!(
                                "Parsed {} formats from VS interface specific descriptors",
                                parsed_formats.len()
                            );
                            formats.extend(parsed_formats);
                        }
                    }
                    Err(e) => {
                        debug!("Failed to get VS interface descriptor: {e:?}");
                    }
                }
            }
        }

        // 如果没有解析到格式，返回一些默认格式
        if formats.is_empty() {
            debug!("No formats parsed from descriptors, using defaults");
            formats = self.get_default_formats();
        }

        Ok(formats)
    }

    /// 获取默认的视频格式
    fn get_default_formats(&self) -> Vec<VideoFormat> {
        vec![
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
        ]
    }

    /// 通过控制请求获取完整的配置描述符
    async fn get_full_configuration_descriptor(&mut self) -> Result<Vec<u8>, USBError> {
        let setup = ControlSetup {
            request_type: RequestType::Standard,
            recipient: Recipient::Device,
            request: Request::GetDescriptor,
            value: (0x02 << 8), // Configuration descriptor type
            index: 0,           // Configuration index
        };

        // 首先获取配置描述符头来确定总长度
        let mut header_buffer = vec![0u8; 9]; // 配置描述符头是9字节
        let transfer = self
            .video_control_interface
            .control_in(setup, &mut header_buffer)?;
        transfer.await?;

        if header_buffer.len() < 4 {
            return Err(USBError::Other(
                "Configuration descriptor header too short".into(),
            ));
        }

        // 提取总长度（小端格式）
        let total_length = u16::from_le_bytes([header_buffer[2], header_buffer[3]]) as usize;
        debug!("Configuration descriptor total length: {total_length} bytes");

        if total_length < 9 {
            return Err(USBError::Other(
                "Invalid configuration descriptor length".into(),
            ));
        }

        // 获取完整的配置描述符
        let mut full_buffer = vec![0u8; total_length];
        let setup_full = ControlSetup {
            request_type: RequestType::Standard,
            recipient: Recipient::Device,
            request: Request::GetDescriptor,
            value: (0x02 << 8), // Configuration descriptor type
            index: 0,           // Configuration index
        };

        let transfer = self
            .video_control_interface
            .control_in(setup_full, &mut full_buffer)?;
        transfer.await?;

        Ok(full_buffer)
    }

    /// 解析VS接口描述符中的格式信息
    fn parse_vs_interface_descriptors(
        &self,
        config_data: &[u8],
        vs_interface_num: u8,
    ) -> Result<Vec<VideoFormat>, USBError> {
        let mut formats = Vec::new();
        let mut pos = 0;
        let mut found_vs_interface = false;
        let mut current_format_type: Option<CurrentFormatType> = None;

        debug!(
            "Parsing configuration descriptor of {} bytes for VS interface {}",
            config_data.len(),
            vs_interface_num
        );

        // 解析配置描述符
        while pos < config_data.len() {
            if pos + 2 > config_data.len() {
                break;
            }

            let length = config_data[pos] as usize;
            let descriptor_type = config_data[pos + 1];

            if length < 2 || pos + length > config_data.len() {
                pos += 1; // 尝试恢复解析
                continue;
            }

            match descriptor_type {
                0x04 => {
                    // Interface descriptor
                    if length >= 9 {
                        let interface_number = config_data[pos + 2];
                        let alternate_setting = config_data[pos + 3];
                        let interface_class = config_data[pos + 5];
                        let interface_subclass = config_data[pos + 6];

                        debug!(
                            "Found interface {interface_number} alt {alternate_setting} class {interface_class} subclass {interface_subclass}"
                        );

                        // 检查是否是我们要找的VS接口 (class=14, subclass=2)
                        if interface_number == vs_interface_num
                            && interface_class == 14
                            && interface_subclass == 2
                        {
                            found_vs_interface = true;
                            debug!("Found target VS interface {vs_interface_num}");
                        } else {
                            found_vs_interface = false;
                        }
                    }
                }
                0x24 => {
                    // Class-specific interface descriptor
                    if found_vs_interface && length >= 3 {
                        let subtype = config_data[pos + 2];
                        debug!(
                            "Found class-specific descriptor subtype 0x{subtype:02x} length {length}"
                        );

                        match subtype {
                            uvc_interface_subtypes::VS_FORMAT_MJPEG => {
                                debug!("Parsing MJPEG format descriptor");
                                current_format_type = Some(CurrentFormatType::Mjpeg);
                            }
                            uvc_interface_subtypes::VS_FORMAT_UNCOMPRESSED => {
                                debug!("Parsing uncompressed format descriptor");
                                if let Ok(format_type) = self
                                    .parse_uncompressed_format_type(&config_data[pos..pos + length])
                                {
                                    current_format_type =
                                        Some(CurrentFormatType::Uncompressed(format_type));
                                }
                            }
                            uvc_interface_subtypes::VS_FORMAT_H264 => {
                                debug!("Found H264 format descriptor");
                                current_format_type = Some(CurrentFormatType::H264);
                            }
                            uvc_interface_subtypes::VS_FRAME_MJPEG
                            | uvc_interface_subtypes::VS_FRAME_UNCOMPRESSED => {
                                debug!("Parsing frame descriptor subtype 0x{subtype:02x}");
                                if let Some(ref format_type) = current_format_type
                                    && let Ok(frame_formats) = self.parse_frame_descriptor(
                                        &config_data[pos..pos + length],
                                        format_type,
                                    )
                                {
                                    formats.extend(frame_formats);
                                }
                            }
                            _ => {
                                debug!("Unknown VS descriptor subtype: 0x{subtype:02x}");
                            }
                        }
                    }
                }
                _ => {
                    // 其他描述符类型，跳过
                }
            }

            pos += length;
        }

        debug!(
            "Parsed {} video formats from VS interface descriptors",
            formats.len()
        );
        Ok(formats)
    }

    /// 解析未压缩格式类型（仅返回格式类型，不生成VideoFormat）
    fn parse_uncompressed_format_type(&self, data: &[u8]) -> Result<UncompressedFormat, USBError> {
        if data.len() < 27 {
            return Err(USBError::Other(
                "Uncompressed format descriptor too short".into(),
            ));
        }

        let guid = &data[5..21];
        debug!("Format GUID: {guid:02x?}");

        // 根据GUID确定格式类型
        let format_type = if guid == uvc_guids::YUY2 {
            debug!("Detected YUY2 format");
            UncompressedFormat::Yuy2
        } else if guid == uvc_guids::NV12 {
            debug!("Detected NV12 format");
            UncompressedFormat::Nv12
        } else if guid == uvc_guids::RGB24 {
            debug!("Detected RGB24 format");
            UncompressedFormat::Rgb24
        } else {
            debug!("Unknown uncompressed format GUID: {guid:02x?}, defaulting to YUY2");
            UncompressedFormat::Yuy2 // 默认为YUY2
        };

        Ok(format_type)
    }

    /// 解析帧描述符
    fn parse_frame_descriptor(
        &self,
        data: &[u8],
        format_type: &CurrentFormatType,
    ) -> Result<Vec<VideoFormat>, USBError> {
        if data.len() < 26 {
            return Err(USBError::Other("Frame descriptor too short".into()));
        }

        let frame_index = data[3];
        let capabilities = data[4];
        let width = u16::from_le_bytes([data[5], data[6]]);
        let height = u16::from_le_bytes([data[7], data[8]]);
        let min_bit_rate = u32::from_le_bytes([data[9], data[10], data[11], data[12]]);
        let max_bit_rate = u32::from_le_bytes([data[13], data[14], data[15], data[16]]);
        let max_video_frame_buffer_size =
            u32::from_le_bytes([data[17], data[18], data[19], data[20]]);
        let default_frame_interval = u32::from_le_bytes([data[21], data[22], data[23], data[24]]);
        let frame_interval_type = data[25];

        debug!(
            "Frame {frame_index}: {width}x{height}, caps=0x{capabilities:02x}, bitrate={min_bit_rate}-{max_bit_rate}, buffer_size={max_video_frame_buffer_size}, default_interval={default_frame_interval}, interval_type={frame_interval_type}"
        );

        // 计算默认帧率 (frame interval 以100ns为单位)
        let default_frame_rate = if default_frame_interval > 0 {
            10_000_000 / default_frame_interval // 转换为fps
        } else {
            30 // 默认帧率
        };

        // 根据格式类型创建VideoFormat
        let video_format = match format_type {
            CurrentFormatType::Mjpeg => VideoFormat::Mjpeg {
                width,
                height,
                frame_rate: default_frame_rate,
            },
            CurrentFormatType::Uncompressed(uncompressed_type) => VideoFormat::Uncompressed {
                width,
                height,
                frame_rate: default_frame_rate,
                format_type: uncompressed_type.clone(),
            },
            CurrentFormatType::H264 => VideoFormat::H264 {
                width,
                height,
                frame_rate: default_frame_rate,
            },
        };

        debug!("Parsed frame format: {video_format:?}");
        Ok(vec![video_format])
    }

    /// 通过控制请求获取VS接口描述符
    async fn get_vs_interface_descriptor(
        &mut self,
        interface_num: u8,
    ) -> Result<Vec<u8>, USBError> {
        let setup = ControlSetup {
            request_type: RequestType::Standard,
            recipient: Recipient::Interface,
            request: Request::GetDescriptor,
            value: (0x04 << 8), // Interface descriptor type
            index: interface_num as u16,
        };

        let mut buffer = vec![0u8; 1024]; // 1KB缓冲区

        // 使用video control接口发送请求
        let transfer = self
            .video_control_interface
            .control_in(setup, &mut buffer)?;
        transfer.await?;

        Ok(buffer)
    }

    /// 解析UVC格式描述符
    fn parse_format_descriptors(&self, data: &[u8]) -> Result<Vec<VideoFormat>, USBError> {
        let mut formats = Vec::new();
        let mut pos = 0;

        while pos < data.len() {
            if pos + 2 > data.len() {
                break;
            }

            let length = data[pos] as usize;
            let descriptor_type = data[pos + 1];

            if length < 3 || pos + length > data.len() {
                break;
            }

            // 检查是否是类特定接口描述符
            if descriptor_type == uvc_descriptor_types::CS_INTERFACE && length >= 3 {
                let subtype = data[pos + 2];

                match subtype {
                    uvc_interface_subtypes::VS_FORMAT_MJPEG => {
                        debug!("Found MJPEG format descriptor");
                        if let Ok(mjpeg_formats) = self.parse_mjpeg_format(&data[pos..pos + length])
                        {
                            formats.extend(mjpeg_formats);
                        }
                    }
                    uvc_interface_subtypes::VS_FORMAT_UNCOMPRESSED => {
                        debug!("Found uncompressed format descriptor");
                        if let Ok(uncompressed_formats) =
                            self.parse_uncompressed_format(&data[pos..pos + length])
                        {
                            formats.extend(uncompressed_formats);
                        }
                    }
                    uvc_interface_subtypes::VS_FORMAT_H264 => {
                        debug!("Found H264 format descriptor");
                        // H264格式解析可以在这里添加
                    }
                    _ => {
                        debug!("Unknown format descriptor subtype: 0x{subtype:02x}");
                    }
                }
            }

            pos += length;
        }

        Ok(formats)
    }

    /// 解析MJPEG格式描述符
    fn parse_mjpeg_format(&self, data: &[u8]) -> Result<Vec<VideoFormat>, USBError> {
        if data.len() < 11 {
            return Err(USBError::Other("MJPEG format descriptor too short".into()));
        }

        let format_index = data[3];
        let num_frame_descriptors = data[4];
        let flags = data[5];
        let default_frame_index = data[6];
        let aspect_ratio_x = data[7];
        let aspect_ratio_y = data[8];
        let interlace_flags = data[9];
        let copy_protect = data[10];

        debug!(
            "MJPEG format: index={format_index}, frames={num_frame_descriptors}, flags=0x{flags:02x}, default_frame={default_frame_index}, aspect={aspect_ratio_x}:{aspect_ratio_y}, interlace=0x{interlace_flags:02x}, copy_protect=0x{copy_protect:02x}"
        );

        // 返回一些基于实际描述符信息的MJPEG格式
        // 在完整实现中，应该继续解析后续的帧描述符来获取具体的分辨率和帧率
        let mut formats = Vec::new();

        // 添加一些常见的MJPEG分辨率，实际应该从帧描述符中解析
        for &(width, height) in &[(640, 480), (1280, 720), (1920, 1080)] {
            formats.push(VideoFormat::Mjpeg {
                width,
                height,
                frame_rate: 30, // 默认帧率，实际应该从帧描述符解析
            });
        }

        debug!(
            "Generated {} MJPEG formats based on format descriptor",
            formats.len()
        );
        Ok(formats)
    }

    /// 解析未压缩格式描述符
    fn parse_uncompressed_format(&self, data: &[u8]) -> Result<Vec<VideoFormat>, USBError> {
        if data.len() < 27 {
            return Err(USBError::Other(
                "Uncompressed format descriptor too short".into(),
            ));
        }

        let format_index = data[3];
        let num_frame_descriptors = data[4];
        let guid = &data[5..21];
        let bits_per_pixel = data[21];
        let default_frame_index = data[22];
        let aspect_ratio_x = data[23];
        let aspect_ratio_y = data[24];
        let interlace_flags = data[25];
        let copy_protect = data[26];

        debug!(
            "Uncompressed format: index={format_index}, frames={num_frame_descriptors}, bpp={bits_per_pixel}, default_frame={default_frame_index}, aspect={aspect_ratio_x}:{aspect_ratio_y}, interlace=0x{interlace_flags:02x}, copy_protect=0x{copy_protect:02x}"
        );

        debug!("Format GUID: {guid:02x?}");

        // 根据GUID确定格式类型
        let format_type = if guid == uvc_guids::YUY2 {
            debug!("Detected YUY2 format");
            UncompressedFormat::Yuy2
        } else if guid == uvc_guids::NV12 {
            debug!("Detected NV12 format");
            UncompressedFormat::Nv12
        } else if guid == uvc_guids::RGB24 {
            debug!("Detected RGB24 format");
            UncompressedFormat::Rgb24
        } else {
            debug!("Unknown uncompressed format GUID: {guid:02x?}, defaulting to YUY2");
            UncompressedFormat::Yuy2 // 默认为YUY2
        };

        // 返回一些基于实际描述符信息的未压缩格式
        // 在完整实现中，应该继续解析后续的帧描述符来获取具体的分辨率和帧率
        let mut formats = Vec::new();

        // 添加一些常见的分辨率，实际应该从帧描述符中解析
        for &(width, height) in &[(320, 240), (640, 480), (1280, 720)] {
            formats.push(VideoFormat::Uncompressed {
                width,
                height,
                frame_rate: 30, // 默认帧率，实际应该从帧描述符解析
                format_type: format_type.clone(),
            });
        }

        debug!(
            "Generated {} uncompressed formats based on format descriptor",
            formats.len()
        );
        Ok(formats)
    }

    /// 设置视频格式
    pub async fn set_format(&mut self, format: VideoFormat) -> Result<(), USBError> {
        // 在实际实现中，这里应该向设备发送 SET_CUR 控制请求
        // 设置 VS_COMMIT_CONTROL 来配置视频格式
        debug!("Setting video format: {format:?}");

        self.current_format = Some(format);
        Ok(())
    }

    /// 开始视频流传输
    pub async fn start_streaming(&mut self) -> Result<(), USBError> {
        if self.ep_in.is_some() {
            // 如果已经有端点，直接开始流传输
            if self.current_format.is_none() {
                return Err(USBError::Other("No format selected".into()));
            }
            self.state = UvcDeviceState::Streaming;
            return Ok(());
        }

        // 如果没有端点，需要切换到有带宽的 alternate setting
        let vs_interface_num = self
            .video_streaming_interface_num
            .ok_or(USBError::NotFound)?;

        if self.current_format.is_none() {
            return Err(USBError::Other("No format selected".into()));
        }

        // 查找有带宽的 alternate setting
        let config = &self.device.configurations[0];
        let vs_interface_group = config
            .interfaces
            .iter()
            .find(|iface| iface.first_alt_setting().interface_number == vs_interface_num)
            .ok_or(USBError::NotFound)?;

        // 查找有 isochronous 端点的 alternate setting
        let mut target_alt_setting = None;
        for alt_setting in vs_interface_group.alt_settings.iter() {
            for endpoint in &alt_setting.endpoints {
                if matches!(endpoint.transfer_type, EndpointType::Isochronous)
                    && matches!(endpoint.direction, Direction::In)
                    && endpoint.max_packet_size > 0
                {
                    target_alt_setting = Some(alt_setting.alternate_setting);
                    break;
                }
            }
            if target_alt_setting.is_some() {
                break;
            }
        }

        let alt_setting = target_alt_setting.ok_or(USBError::NotFound)?;

        debug!("Switching to alternate setting {alt_setting} for streaming");

        // 切换到有带宽的 alternate setting
        let mut vs_interface = self
            .device
            .claim_interface(vs_interface_num, alt_setting)
            .await?;

        // 查找同步 IN 端点
        for endpoint in vs_interface.descriptor.endpoints.clone().into_iter() {
            if matches!(endpoint.transfer_type, EndpointType::Isochronous)
                && matches!(endpoint.direction, Direction::In)
            {
                debug!("Found isochronous IN endpoint: {endpoint:?}");
                self.ep_in = Some(vs_interface.endpoint_iso_in(endpoint.address)?);
                break;
            }
        }

        self.video_streaming_interface = Some(vs_interface);

        debug!("Starting video streaming");
        self.state = UvcDeviceState::Streaming;
        Ok(())
    }

    /// 停止视频流传输
    pub async fn stop_streaming(&mut self) -> Result<(), USBError> {
        debug!("Stopping video streaming");

        // 切换回 alternate setting 0（无带宽）
        if let Some(vs_interface_num) = self.video_streaming_interface_num {
            let vs_interface = self.device.claim_interface(vs_interface_num, 0).await?;
            self.video_streaming_interface = Some(vs_interface);
        }

        // 清除端点引用
        self.ep_in = None;
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

        // UVC 设备使用基于端点最大包大小的缓冲区
        // 对于 isochronous 端点，我们需要多个包的空间
        let mut buf = vec![0u8; 4096]; // 使用更大的缓冲区

        // 尝试接收多个包来获取完整帧
        for _attempt in 0..10 {
            match ep_in.submit(&mut buf, 1)?.await {
                Ok(n) => {
                    if n == 0 {
                        continue; // 没有数据，继续尝试
                    }

                    let data = &buf[..n];
                    debug!("Received {n} bytes from USB");

                    // UVC 视频数据包格式分析
                    if data.len() < 2 {
                        continue;
                    }

                    // UVC 载荷头分析 (简化版本)
                    let header_length = data[0] as usize;
                    if header_length > data.len() || header_length < 2 {
                        debug!("Invalid header length: {header_length}");
                        continue;
                    }

                    let header_info = data[1];
                    let _frame_id = (header_info & 0x01) != 0;
                    let end_of_frame = (header_info & 0x02) != 0;
                    let presentation_time = (header_info & 0x04) != 0;
                    let _source_clock_ref = (header_info & 0x08) != 0;
                    let error = (header_info & 0x40) != 0;

                    if error {
                        debug!("UVC payload error detected");
                        continue;
                    }

                    // 提取实际的视频数据（跳过载荷头）
                    let payload_data = &data[header_length..];

                    // 将数据添加到帧缓冲区
                    if !payload_data.is_empty() {
                        self.frame_buffer.extend_from_slice(payload_data);
                        debug!(
                            "Added {} bytes to frame buffer (total: {})",
                            payload_data.len(),
                            self.frame_buffer.len()
                        );
                    }

                    if end_of_frame && !self.frame_buffer.is_empty() {
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
                        return Ok(Some(frame));
                    }
                }
                Err(e) => {
                    debug!("Transfer error: {e:?}");
                    return Err(e);
                }
            }
        }

        // 如果循环结束还没有完整帧，返回None
        Ok(None)
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
        debug!("Sending video control command: {command:?}");

        let processing_unit_id = self.processing_unit_id.ok_or(USBError::NotFound)?;

        match command {
            VideoControlEvent::BrightnessChanged(value) => {
                debug!("Setting brightness to: {value}");
                self.send_pu_control(
                    pu_controls::PU_BRIGHTNESS_CONTROL,
                    processing_unit_id,
                    &value.to_le_bytes(),
                )
                .await?;
            }
            VideoControlEvent::ContrastChanged(value) => {
                debug!("Setting contrast to: {value}");
                self.send_pu_control(
                    pu_controls::PU_CONTRAST_CONTROL,
                    processing_unit_id,
                    &(value as u16).to_le_bytes(),
                )
                .await?;
            }
            VideoControlEvent::HueChanged(value) => {
                debug!("Setting hue to: {value}");
                self.send_pu_control(
                    pu_controls::PU_HUE_CONTROL,
                    processing_unit_id,
                    &value.to_le_bytes(),
                )
                .await?;
            }
            VideoControlEvent::SaturationChanged(value) => {
                debug!("Setting saturation to: {value}");
                self.send_pu_control(
                    pu_controls::PU_SATURATION_CONTROL,
                    processing_unit_id,
                    &(value as u16).to_le_bytes(),
                )
                .await?;
            }
            _ => {
                debug!("Control command not implemented: {command:?}");
            }
        }

        Ok(())
    }

    /// 发送处理单元控制请求
    async fn send_pu_control(
        &mut self,
        control_selector: u8,
        unit_id: u8,
        data: &[u8],
    ) -> Result<(), USBError> {
        let setup = ControlSetup {
            request_type: RequestType::Class,
            recipient: Recipient::Interface,
            request: uvc_requests::SET_CUR.into(),
            value: (control_selector as u16) << 8,
            index: unit_id as u16,
        };

        self.video_control_interface
            .control_out(setup, data)
            .await?
            .await?;

        Ok(())
    }

    /// 获取设备信息字符串
    pub async fn get_device_info(&self) -> Result<String, USBError> {
        // 在实际实现中，这里可以读取设备的字符串描述符
        Ok("UVC Video Device".to_string())
    }
}
