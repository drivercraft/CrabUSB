use crab_usb::USBHost;
use env_logger;
use log::{error, info, warn};
use std::{hint::spin_loop, thread, time::Duration};
use tokio::time;
use usb_uvc::{UvcDevice, UvcDeviceState, VideoControlEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    info!("Starting UVC video capture example");

    // 创建 USB 主机
    let mut host = USBHost::new_libusb();
    let event_handler = host.event_handler();
    thread::spawn(move || {
        while event_handler.handle_event() {
            spin_loop();
        }
    });

    // 扫描连接的设备
    let devices = host.device_list().await?;

    // 查找 UVC 设备
    let mut uvc_device = None;
    for mut device_info in devices {
        info!(
            "Checking device: VID={:04x}, PID={:04x}",
            device_info.vendor_id(),
            device_info.product_id()
        );

        if UvcDevice::check(&device_info) {
            info!("Found UVC device!");
            let device = device_info.open().await?;
            uvc_device = Some(UvcDevice::new(device).await?);
            break;
        }
    }

    let mut uvc = match uvc_device {
        Some(device) => device,
        None => {
            warn!("No UVC device found. Make sure a USB camera is connected.");
            return Ok(());
        }
    };

    // 获取设备信息
    let device_info = uvc.get_device_info().await?;
    info!("Device info: {}", device_info);

    // 获取支持的视频格式
    let formats = uvc.get_supported_formats().await?;
    info!("Supported formats:");
    for format in &formats {
        info!("  {:?}", format);
    }

    // 设置视频格式 (选择第一个可用格式)
    if let Some(format) = formats.first() {
        info!("Setting format: {:?}", format);
        uvc.set_format(format.clone()).await?;
    } else {
        error!("No supported formats available");
        return Ok(());
    }

    // 开始视频流
    info!("Starting video streaming...");
    uvc.start_streaming().await?;

    // 设置一些控制参数的示例
    info!("Setting video controls...");

    // 尝试设置亮度（如果失败也继续）
    if let Err(e) = uvc
        .send_control_command(VideoControlEvent::BrightnessChanged(100))
        .await
    {
        warn!("Failed to set brightness: {:?}", e);
    }

    // 尝试设置对比度（如果失败也继续）
    // if let Err(e) = uvc
    //     .send_control_command(VideoControlEvent::ContrastChanged(50))
    //     .await
    // {
    //     warn!("Failed to set contrast: {:?}", e);
    // }

    let mut frame_count = 0;
    let start_time = std::time::Instant::now();

    // 捕获视频帧 (运行30秒)
    info!("Capturing video frames for 30 seconds...");
    let capture_duration = Duration::from_secs(30);
    let mut last_stats_time = std::time::Instant::now();

    while start_time.elapsed() < capture_duration {
        match uvc.recv_frame().await {
            Ok(Some(frame)) => {
                frame_count += 1;

                // 每10秒打印一次统计信息
                if last_stats_time.elapsed() >= Duration::from_secs(10) {
                    let fps = frame_count as f32 / start_time.elapsed().as_secs_f32();
                    info!(
                        "Captured {} frames, average FPS: {:.2}, last frame size: {} bytes",
                        frame_count,
                        fps,
                        frame.data.len()
                    );
                    last_stats_time = std::time::Instant::now();
                }

                // 在实际应用中，这里可以处理视频帧数据：
                // - 解码 MJPEG/H.264 数据
                // - 转换颜色格式
                // - 保存到文件或显示到屏幕
                // - 进行计算机视觉处理等

                // 示例：保存前几帧到文件
                if frame_count <= 5 {
                    save_frame_to_file(&frame, frame_count).await?;
                }
            }
            Ok(None) => {
                // 没有完整帧可用，继续等待
                time::sleep(Duration::from_millis(1)).await;
            }
            Err(e) => {
                warn!("Error receiving frame: {:?}", e);
                time::sleep(Duration::from_millis(10)).await;
            }
        }

        // 检查设备状态
        if *uvc.get_state() != UvcDeviceState::Streaming {
            error!("Device is no longer streaming");
            break;
        }
    }

    // 停止视频流
    info!("Stopping video streaming...");
    uvc.stop_streaming().await?;

    let total_time = start_time.elapsed();
    let avg_fps = frame_count as f32 / total_time.as_secs_f32();
    info!(
        "Capture completed. Total frames: {}, Average FPS: {:.2}",
        frame_count, avg_fps
    );

    Ok(())
}

async fn save_frame_to_file(
    frame: &usb_uvc::VideoFrame,
    frame_number: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    use tokio::fs::File;
    use tokio::io::AsyncWriteExt;

    let filename = format!("frame_{:03}.raw", frame_number);
    let mut file = File::create(&filename).await?;
    file.write_all(&frame.data).await?;
    info!("Saved frame {} to {}", frame_number, filename);
    Ok(())
}
