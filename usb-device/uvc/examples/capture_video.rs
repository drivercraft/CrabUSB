use crab_usb::USBHost;
use env_logger;
use log::{error, info, warn};
use std::{hint::spin_loop, sync::Arc, thread, time::Duration};
use tokio::time;
use usb_uvc::{UvcDevice, UvcDeviceState, VideoControlEvent, frame};

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
    let mut stream = uvc.start_streaming().await?;

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

    let start_time = std::time::Instant::now();

    // 捕获视频帧 (运行30秒)
    info!("Capturing video frames for 30 seconds...");
    let capture_duration = Duration::from_secs(6);
    let frame_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let frame_count_clone = frame_count.clone();

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();
    let handle = tokio::spawn(async move {
        // 处理设备事件
        while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
            let data = stream.recv().await;
            match data {
                Ok(frame) => {
                    frame_count_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
                Err(e) => {
                    warn!("Error receiving frame: {:?}", e);
                }
            }
            ()
        }
    });

    tokio::time::sleep(capture_duration).await;

    running.store(false, std::sync::atomic::Ordering::Relaxed);
    handle.await.unwrap();

    let frame_count = frame_count.load(std::sync::atomic::Ordering::Acquire);

    let avg_fps = frame_count as f32 / start_time.elapsed().as_secs_f32();
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
