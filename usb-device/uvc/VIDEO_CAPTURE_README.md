# UVC 视频捕获示例

## 概述

这个示例展示如何使用 usb-uvc 库捕获 UVC (USB Video Class) 设备的视频流，并将捕获的帧转换为视频文件。

## 功能特性

- **自动设备检测**: 自动检测并连接第一个可用的 UVC 设备
- **多格式支持**: 支持 MJPEG、H.264、YUY2 等多种视频格式
- **智能格式选择**: 根据流的实际格式自动选择最佳转换方法
- **Rust-native 转换**: 使用纯 Rust 代码进行视频转换，无需外部脚本
- **帧缓存**: 智能缓存机制，避免内存溢出
- **异步处理**: 完全异步的视频捕获和处理

## 依赖要求

- **FFmpeg**: 系统需要安装 FFmpeg，用于视频编码和格式转换
- **libusb**: USB 设备访问库
- **Rust**: 支持 async/await 的 Rust 版本

## 使用方法

### 基本运行

```bash
# 在项目根目录下运行
cargo run --example capture_video
```

### 高级选项

```bash
# 指定捕获时长（秒）
CAPTURE_DURATION=30 cargo run --example capture_video

# 启用详细日志
RUST_LOG=debug cargo run --example capture_video
```

## 输出文件

程序会在当前目录下创建以下文件：

- `output_YYYYMMDD_HHMMSS.mp4`: 最终的视频文件
- `frames/`: 临时帧文件目录（捕获完成后自动清理）
- `stream_info.txt`: 流格式信息（可选）

## 视频转换流程

### MJPEG 格式

1. 直接保存 JPEG 帧到文件
2. 使用 FFmpeg 将 JPEG 序列转换为 MP4

### H.264 格式

1. 提取 H.264 NAL 单元
2. 构建完整的 H.264 流
3. 使用 FFmpeg 封装为 MP4 容器

### YUY2/其他格式

1. 保存原始帧数据
2. 转换为 PNG 图像序列
3. 使用 FFmpeg 编码为 MP4

## 技术实现

### Rust-native 转换

所有视频转换现在都使用纯 Rust 代码实现：

```rust
// 使用 tokio::spawn_blocking 进行 CPU 密集型操作
let result = spawn_blocking(move || {
    Command::new("ffmpeg")
        .args(&args)
        .output()
}).await??;
```

### 格式检测

程序会自动检测流格式并选择最佳转换方法：

```rust
match stream.video_format.subtype {
    b"MJPG" => create_video_from_mjpeg_frames(frame_dir, output_path).await?,
    b"H264" => create_video_from_h264_frames(frame_dir, output_path).await?,
    _ => create_video_from_images(frame_dir, output_path).await?,
}
```

## 支持的格式

### 未压缩格式

- **YUY2 (YUYV)**：最常见的 UVC 格式，每像素 2 字节
- **RGB24**：24 位 RGB 格式，每像素 3 字节
- **RGB32 (RGBA)**：32 位 RGBA 格式，每像素 4 字节
- **NV12**：YUV 4:2:0 格式，每像素 1.5 字节

### 压缩格式

- **MJPEG**：Motion JPEG 压缩格式
- **H.264**：高效视频压缩格式

## 性能优化

- **异步 I/O**: 所有文件操作都是异步的
- **智能缓存**: 限制内存中的帧数量
- **并行处理**: 使用 spawn_blocking 避免阻塞异步运行时
- **格式优化**: 根据输入格式选择最效率的转换路径

## 故障排除

### 常见问题

1. **设备权限**: 确保对 USB 设备有访问权限

   ```bash
   sudo usermod -a -G dialout $USER
   # 或者使用 sudo 运行
   ```

2. **FFmpeg 未安装**:

   ```bash
   # Ubuntu/Debian
   sudo apt install ffmpeg
   
   # macOS
   brew install ffmpeg
   ```

3. **设备已被占用**: 确保没有其他程序在使用摄像头

### 调试选项

```bash
# 启用详细日志
RUST_LOG=trace cargo run --example capture_video

# 保留临时文件用于调试
DEBUG_KEEP_FRAMES=1 cargo run --example capture_video
```

## 技术细节

### 帧解析

程序使用自定义的帧解析器处理 UVC 负载：

- 检测帧边界
- 处理分片传输
- 验证帧完整性

### 内存管理

- 使用 `Vec<u8>` 进行高效的字节操作
- 实现智能缓存避免 OOM
- 及时释放已处理的帧

### 错误处理

- 完整的错误传播链
- 详细的错误信息
- 优雅的资源清理

## 开发说明

### 添加新格式支持

1. 在 `create_video_from_frames` 中添加新的格式分支
2. 实现对应的转换函数
3. 更新格式检测逻辑

### 性能调优

- 调整帧缓存大小 (`MAX_CACHED_FRAMES`)
- 优化 FFmpeg 参数
- 考虑硬件加速选项

## 示例输出

```
INFO  Starting UVC video capture example
INFO  Found UVC device!
INFO  Current video format: Uncompressed { width: 640, height: 480, frame_rate: 30, format_type: Yuy2 }
INFO  Starting video streaming...
INFO  Capturing video frames for 6 seconds...
INFO  Capture completed. Total frames: 180, Average FPS: 30.00
INFO  Converting frames to video using Rust-native implementation...
INFO  Video created successfully!
INFO  Video saved as output_20231225_143022.mp4
```

这个实现提供了完整的 UVC 视频捕获和转换流程，使用纯 Rust 代码进行视频转换，无需外部脚本依赖。
