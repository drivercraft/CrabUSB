# ISO传输参数计算实现

本文档说明了如何参考libusb的stream.c实现，改进CrabUSB中UVC设备的ISO传输缓冲区大小和endpoint_bytes_per_packet的计算。

## 参考实现：libusb stream.c

基于libusb/libuvc项目中的`stream.c`文件，我们学习了以下关键计算逻辑：

### 1. 端点包大小计算 (endpoint_bytes_per_packet)

```rust
// 根据libusb的计算方法:
// wMaxPacketSize: [unused:2 (multiplier-1):3 size:11]
let raw_packet_size = endpoint.max_packet_size as usize;
let packet_size = (raw_packet_size & 0x07ff) * (((raw_packet_size >> 11) & 3) + 1);
```

这个计算方法：

- 提取低11位作为基本包大小
- 提取第11-12位作为额外事务倍数（USB 2.0高速端点特性）
- 最终包大小 = 基本大小 × (倍数 + 1)

### 2. 每次传输的包数量计算 (packets_per_transfer)

```rust
// packets_per_transfer = (dwMaxVideoFrameSize + endpoint_bytes_per_packet - 1) / endpoint_bytes_per_packet
// 但保持合理的限制(最多32个包)
let packets_per_transfer = std::cmp::min(
    (max_video_frame_size + endpoint_bytes_per_packet - 1) / endpoint_bytes_per_packet,
    32
);
```

这个计算确保：

- 能容纳最大视频帧大小
- 限制在合理范围内（≤32包），避免传输超时
- 使用向上取整除法

### 3. 总传输缓冲区大小计算

```rust
let total_transfer_size = packets_per_transfer * endpoint_bytes_per_packet;
```

简单的乘法，确保缓冲区能容纳所有包。

### 4. 不同视频格式的帧大小估算

```rust
let (max_video_frame_size, _) = match &self.current_format {
    Some(VideoFormat::Mjpeg { width, height, .. }) => {
        // MJPEG压缩率约2:1
        let max_frame_size = (*width as usize * *height as usize * 3) / 2;
        (max_frame_size, 614400_u32 as usize)
    }
    Some(VideoFormat::Uncompressed { width, height, format_type, .. }) => {
        let bytes_per_pixel = match format_type {
            UncompressedFormat::Yuy2 => 2, // YUY2是16位每像素
            UncompressedFormat::Nv12 => 1, // NV12平均每像素12位
            _ => 2,
        };
        let max_frame_size = *width as usize * *height as usize * bytes_per_pixel;
        (max_frame_size, 614400_u32 as usize)
    }
    Some(VideoFormat::H264 { width, height, .. }) => {
        // H264压缩率约8:1
        let max_frame_size = (*width as usize * *height as usize) / 4;
        (max_frame_size, 614400_u32 as usize)
    }
    None => (640 * 480 * 2, 614400), // 默认值
};
```

## 实现优势

### 1. 动态缓冲区大小

- 根据实际视频格式和分辨率计算
- 避免固定大小导致的内存浪费或缓冲区不足

### 2. 准确的端点包大小

- 考虑USB 2.0高速端点的额外事务倍数
- 直接从设备描述符获取实际值

### 3. 合理的传输参数

- 限制包数量避免传输超时
- 向上取整确保能容纳完整帧

### 4. 格式感知的计算

- 针对不同压缩格式使用不同的估算方法
- MJPEG、H264等压缩格式使用合理的压缩比

## 使用示例

```rust
// 接收视频帧时，自动使用优化的ISO传输参数
match uvc_device.recv_frame().await {
    Ok(Some(frame)) => {
        println!("Received frame: {} bytes", frame.data.len());
    }
    Ok(None) => println!("No frame available"),
    Err(e) => println!("Transfer error: {:?}", e),
}
```

系统会自动：

1. 根据当前视频格式计算最大帧大小
2. 获取当前端点的实际包大小
3. 计算合适的包数量和总缓冲区大小
4. 使用这些参数进行ISO传输

## 调试信息

实现中包含详细的调试日志：

```
DEBUG: Current endpoint packet size: 1024 (raw: 1024)
DEBUG: ISO transfer params: packets_per_transfer=16, endpoint_bytes_per_packet=1024, total_transfer_size=16384
DEBUG: Video format params: max_video_frame_size=460800, max_payload_transfer_size=614400
```

这些信息有助于诊断传输问题和验证计算结果。

## 总结

通过参考libusb的成熟实现，我们的ISO传输参数计算变得更加智能和可靠：

- **更准确**：基于实际设备特性和视频格式
- **更高效**：避免不必要的内存分配
- **更可靠**：参考经过广泛测试的libusb实现
- **更灵活**：支持多种视频格式和设备配置

这些改进将显著提升UVC设备的视频流传输性能和稳定性。
