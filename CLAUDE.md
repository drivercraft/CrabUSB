# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## CrabUSB 架构概述

CrabUSB 是一个为嵌入式系统和操作系统内核设计的高性能异步 USB 主机驱动程序，使用 Rust 编写。该项目采用**无锁设计**，基于 TRB (Transfer Request Block) 环形结构，每个 TRB 代表一个异步任务。

### 核心特性

- **Async/Await 支持**: 从头开始使用 async 原语构建非阻塞 USB 操作
- **无锁设计**: 基于 TRB 环形架构，零锁异步操作
- **xHCI 控制器支持**: 完整实现 xHCI (可扩展主机控制器接口) 规范
- **No-STD 兼容**: 为 `#![no_std]` 环境设计
- **多后端支持**: xHCI (直接硬件访问) 和 libusb (用户空间测试)

### 架构关键点

1. **TRB 环形机制**: 每个 TRB 代表一个异步任务，Future 查询环形结构获取结果
2. **执行器无关**: 不绑定特定执行器，可同步使用
3. **DMA 感知**: 高效的内存管理，支持 DMA 一致性
4. **零成本抽象**: 类型安全端点（通过 newtype 模式实现）

### 工作区结构

```
CrabUSB/
├── usb-host/           # 主机驱动核心实现 (crab-usb)
│   └── src/
│       ├── backend/    # 后端实现
│       │   ├── xhci/   # xHCI 硬件驱动
│       │   └── libusb/ # libusb 用户空间后端 (libusb feature)
│       ├── common/     # 通用设备管理
│       └── osal.rs     # OS 抽象层 (Kernel trait)
├── usb-if/             # USB 接口定义和类型
│   └── src/
│       ├── descriptor/ # USB 描述符解析
│       ├── host/       # Host trait 定义 (Controller, Device, Interface, Endpoint)
│       └── transfer/   # 传输类型定义
├── usb-device/         # USB 设备类实现
│   ├── uvc/            # USB Video Class (crab-uvc)
│   └── hid/keyboard/   # HID 键盘设备
├── test_crates/        # 测试用例
│   ├── test_xhci_uvc/  # xHCI UVC 测试 (aarch64-none)
│   ├── test_libusb_uvc/# libusb UVC 测试
│   └── test_libusb/    # libusb 基础测试
└── utils/
    └── uvc-frame-parser/ # UVC 帧解析工具
```

**关键模块职责**:

- `usb-if`: 定义跨后端的统一接口，所有后端必须实现这些 trait
- `usb-host/backend`: 后端实现，xHCI 用于生产环境，libusb 用于开发测试
- `usb-device`: 设备类驱动，UVC 是最复杂的实现（视频流捕获）
- `test_crates`: 验证功能，UVC 测试覆盖从格式协商到视频流捕获的完整流程

## 必须遵守

修改完代码后，确保 `cargo check -p crab-usb --test test --target aarch64-unknown-none-softfloat` 可以通过，
执行 `cargo fmt --all` 保持代码风格一致。

### 依赖管理

- **workspace dependencies**: 在根 `Cargo.toml` 中统一定义版本
- **常用依赖**:

  - `futures` (default-features = false): 异步原语
  - `thiserror` (default-features = false): 错误派生
  - `log`: 日志记录
  - `tock-registers`: 硬件寄存器访问
  - `bare-test`: 裸机测试框架
  - `rockchip-*`: Rockchip 平台支持

- **查询文档**: 使用 `context7` MCP 服务器查询依赖库的使用方法

**主线内核**: `/home/zhourui/linux-la64`
**QEMU**: `/home/zhourui/opensource/qemu-10.1.0`

**注意**: RK3588 的某些寄存器和配置与主线内核不同，需要参考 Orange Pi 的特定实现。

## 常用开发命令

### 构建与测试

```bash
# 标准 no_std 测试 (使用 QEMU aarch64)
cargo test -p crab-usb --test test --target aarch64-unknown-none-softfloat -- -c qemu.toml --show-output

# 运行特定测试 (例如 uboot 测试)
cargo test --package test_xhci_uvc --test test --target aarch64-unknown-none-softfloat -- --show-output uboot

# libusb 后端测试 (需要 libudev-dev)
cargo test -p crab-usb --features libusb --test test

# UVC 帧解析工具
cargo run -p uvc-frame-parser -- -l target/uvc.log -o target/output
```

### 目标平台

- **默认目标**: `aarch64-unknown-none-softfloat`
- **Runner**: `cargo osrun` (由 ostool 提供)
- **配置**: `.cargo/config.toml`

## 代码架构深入理解

### 1. 后端抽象层

`usb-host/src/backend/` 提供了硬件抽象:

- **xHCI**: 直接硬件访问，用于嵌入式系统和 OS 内核

  - `mod.rs`: `Xhci` 结构体实现 `usb_if::host::Controller` trait
  - `ring/`: TRB 环形管理，核心异步机制
  - `event.rs`: 事件处理和中断管理
  - `context.rs`: 设备上下文管理
  - `endpoint.rs`: 端点管理

- **libusb**: 用户空间后端，用于开发和测试
  - 需要 `libusb` feature 启用
  - 使用 `libusb1-sys` 绑定
  - **关键实现细节**：
    - **端点地址**: 必须保留完整的 `bEndpointAddress`（包括方向位 bit 7），不能截断为 `& 0x0F`
    - **ISO transfer**: `libusb_alloc_transfer()` 第一个参数是 `iso_packets` 数量，ISO 传输必须传入正确的 packet 数
    - **控制传输 buffer**: 异步控制传输需要特殊布局 - `[8字节setup包] [数据区]`，必须使用 `temp_buff` 来保存

### 2. USB 接口层 (usb-if)

定义了所有 USB 操作的标准 trait:

```rust
// Controller: 顶层控制器管理
pub trait Controller {
    fn init(&mut self) -> LocalBoxFuture<'_, Result<(), USBError>>;
    fn device_list(&self) -> LocalBoxFuture<'_, Result<Vec<Box<dyn DeviceInfo>>, USBError>>;
    fn handle_event(&mut self);  // 中断上下文调用
}

// Device: USB 设备操作
pub trait Device {
    fn set_configuration(&mut self, configuration: u8) -> ...;
    fn claim_interface(&mut self, interface: u8, alternate: u8) -> ...;
    // Control 传输...
}

// Interface: 接口和端点访问
pub trait Interface {
    fn endpoint_bulk_in(&mut self, endpoint: u8) -> Result<Box<dyn EndpointBulkIn>, USBError>;
    fn endpoint_bulk_out(&mut self, endpoint: u8) -> ...;
    // 其他端点类型...
}
```

### 3. OS 抽象层 (OSAL)

`usb-host/src/osal.rs` 定义了 `Kernel` trait，需要由使用者实现:

```rust
pub trait Kernel {
    fn sleep(duration: Duration) -> BoxFuture<'_, ()>;
    fn page_size() -> usize;
}
```

使用 `trait-ffi` 的 `def_extern_trait` 宏实现 FFI 兼容。

### 4. 异步模型

- **Future 类型**: `LocalBoxFuture<'_, Result<T, E>>`
- **传输返回**: `ResultTransfer<'a> = Result<TransferFuture<'a>, TransferError>`
- **唤醒机制**: 使用 `AtomicWaker` 实现端口状态变化通知
- **执行器无关**: 不绑定特定执行器，可同步使用

### 5. 传输类型

所有传输类型都在 `usb-if/src/transfer/` 中定义:

- **Control**: 设备设置和标准请求
- **Bulk**: 高吞吐量数据传输 (存储设备)
- **Interrupt**: 周期性数据传输 (HID 设备)
- **Isochronous**: 实时流传输 (音频/视频设备)

### 6. 类型安全端点

后端使用零成本抽象确保类型安全:

```rust
pub mod endpoint {
    pub mod kind {
        pub struct Bulk;       // 批量传输
        pub struct Interrupt;  // 中断传输
        pub struct Isochronous; // 等时传输
    }
    pub mod direction {
        pub struct In;   // 读取
        pub struct Out;  // 写入
    }
}
```

## 开发注意事项

### Feature Flags

- `libusb`: 启用 libusb 后端，仅用于非 `target_os = "none"` 目标
- 默认情况下，当 `target_os = "none"` 时自动启用 `no_std`

### 错误处理规范

- **no_std 环境**: 不能使用 `anyhow!` 宏
- **正确方式**: `USBError::from("message")` 或 `"message".into()`
- **避免**: `USBError::Other("...".into())` - 这会导致类型推断问题

### 端点描述符解析

**USB 端点地址格式**:

- Bit 7: 方向位 (1=IN, 0=OUT)
- Bits 3:0: 端点号
- 示例: 端点 1 IN = `0x81` (10000001b)

**wMaxPacketSize 字段解析** (高速等时/中断端点):

- Bits 10:0: 最大包大小
- Bits 12:11: 事务乘数 (transactions_per_microframe = 值 + 1)
- 示例: `0x1400` = max_packet_size=1024, packets_per_microframe=2
- 实际包大小 = `max_packet_size * packets_per_microframe`

**类特定描述符 (extra 字段)**:

- `InterfaceDescriptor::extra` 包含类特定描述符数据（如 UVC 格式/帧描述符）
- 在 `libusb_get_config_descriptor` 中从 `alt_desc.extra` 提取
- 用于避免额外的控制传输

### 内存管理

- 使用 `dma-api` 处理 DMA 一致性
- `crossbeam` 提供 lock-free 数据结构
- 依赖 `alloc` crate，需与分配器集成

### 测试模式

1. **xHCI 测试**: 使用 QEMU aarch64 模拟硬件
2. **libusb 测试**: 在主机 OS 上运行
3. **UVC 测试**: 专门测试 USB Video Class 设备

**UVC 测试运行**:

```bash
# 运行 UVC 相机测试（会捕获 30 秒视频）
cargo run -p test_libusb_uvc

# 输出位置
# - JPEG 帧: target/output/images/frame_*.jpg
# - 视频信息: target/frames/video_info.toml
```

### 代码风格

- 使用 `edition = "2024"`
- 广泛使用 `newtype` 模式 (通过 `define_int_type!` 宏)
- 使用 `num_enum` 实现枚举转换
- 使用 `tock-registers` 进行硬件寄存器访问

## 常见任务

### 添加新的 USB 设备支持

1. 在 `usb-device/` 下创建新包
2. 使用 `crab-usb` 和 `usb-if` 作为依赖
3. 实现 `usb_if::host::Interface` trait 的使用模式

### 实现新的后端

1. 在 `usb-host/src/backend/` 创建新目录
2. 实现 `usb_if::host::Controller` trait
3. 在 `usb-host/src/backend/mod.rs` 中导出

### 调试建议

- 启用 `log` crate 的 debug 级别日志
- 使用 `bare-test` 框架进行裸机测试
- 检查 `test_crates/` 中的示例用法

### 常见陷阱与解决方案

#### 1. libusb 控制传输 Stall

**症状**: PROBE/COMMIT 请求返回 Stall 错误

**原因**: 控制传输 buffer 布局不正确。libusb 异步控制传输需要：

```
[8字节setup包] [数据区]
```

**解决**: 在 `usb-host/src/backend/libusb/endpoint.rs` 中使用 `temp_buff`:

```rust
let temp_buff = vec![0u8; 8 + data_len]; // setup + 数据
// OUT 传输复制数据到 buffer[8..]
libusb_fill_control_setup(buffer, ...); // 填充 buffer[0..8]
```

#### 2. ISO 传输崩溃 (dump core)

**症状**: 程序在 ISO 传输时崩溃

**原因**: `libusb_alloc_transfer(0)` 没有为 ISO packet descriptors 分配内存

**解决**: 根据 transfer 类型传递正确的 packet 数量:

```rust
let iso_packets = match &transfer.kind {
    TransferKind::Isochronous { num_pkgs } => *num_pkgs as i32,
    _ => 0,
};
let trans_ptr = unsafe { libusb_alloc_transfer(iso_packets) };
```

#### 3. 端点地址错误导致传输失败

**症状**: libusb 无法识别端点，传输失败

**原因**: 端点地址被截断 (`& 0x0F`)，丢失方向位

**解决**: 保留完整的端点地址:

```rust
// ❌ 错误
address: ep_desc.bEndpointAddress & 0x0F

// ✅ 正确
address: ep_desc.bEndpointAddress
```

#### 4. 类型推断错误

**症状**: `the trait bound '&str: Into<anyhow::Error> is not satisfied`

**原因**: 在 no_std 环境中使用 `anyhow!` 宏或 `USBError::Other("...".into())`

**解决**: 显式使用 `USBError::from()`:

```rust
// ❌ 错误
USBError::Other("message".into())
anyhow!("message").into()

// ✅ 正确
USBError::from("message")
"message".into()  // 如果上下文类型明确
```
