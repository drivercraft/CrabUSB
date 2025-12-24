# Hub 架构设计文档

## 概述

本文档描述 CrabUSB 中 Root Hub 和 External Hub 的共性接口设计。

## 核心概念

### Hub 层次结构

```
                    ┌──────────────────────────────┐
                    │      USB Host System         │
                    └──────────────────────────────┘
                                    │
                    ┌───────────────┴───────────────┐
                    ▼                               ▼
        ┌───────────────────────┐       ┌──────────────────────┐
        │   Host Controller     │       │   External Hub       │
        │   (xHCI/EHCI/UHCI)    │       │   (USB Device)       │
        └───────────────────────┘       └──────────────────────┘
                    │                               │
        ┌───────────┴───────────┐       ┌───────────┴───────────┐
        ▼                       ▼       ▼                       ▼
   ┌─────────┐           ┌─────────┐ ┌─────────┐           ┌─────────┐
   │ Port 0  │           │ Port N  │ │ Port 0  │           │ Port N  │
   └─────────┘           └─────────┘ └─────────┘           └─────────┘
        │                       │         │                       │
```

### Hub 分类

| 类型 | 位置 | 访问方式 | 传输机制 |
|------|------|----------|----------|
| **Root Hub** | Host Controller 内部 | 直接寄存器访问 | 内存读写 |
| **External Hub** | USB 总线上 | USB Control 传输 | USB 协议 |

## Trait 层次设计

```rust
                    ┌──────────────────────────┐
                    │         Hub              │
                    │  • 端口管理               │
                    │  • 描述符访问             │
                    │  • 状态查询               │
                    └───────────┬──────────────┘
                                │
            ┌───────────────────┴───────────────────┐
            ▼                                       ▼
┌──────────────────────┐               ┌──────────────────────┐
│     RootHub          │               │    ExternalHub       │
│  • 寄存器访问         │               │  • USB 设备接口      │
│  • 中断直接触发       │               │  • 状态变化端点      │
│  • 无需地址分配       │               │  • 需要 TT           │
└──────────────────────┘               └──────────────────────┘
```

## 共性接口详解

### 1. Hub Trait (基础接口)

```rust
pub trait Hub: Send + 'static {
    // 获取 Hub 描述符
    fn hub_descriptor(&self) -> LocalBoxFuture<'_, Result<HubDescriptor, USBError>>;

    // 获取端口数量
    fn num_ports(&self) -> u8;

    // 获取端口操作接口
    fn port(&mut self, port_index: u8) -> Result<Box<dyn HubPortOps>, USBError>;

    // 获取所有端口状态
    fn port_status_all(&mut self) -> LocalBoxFuture<'_, Result<Vec<PortStatus>, USBError>>;

    // Hub 特性
    fn hub_characteristics(&self) -> HubCharacteristics;

    // 电源控制模式
    fn power_switching_mode(&self) -> PowerSwitchingMode;

    // 处理 Hub 事件
    unsafe fn handle_event(&mut self) -> LocalBoxFuture<'_, Result<(), USBError>>;
}
```

**设计要点：**
- 统一端口访问方式（无论 Root/External）
- 异步接口支持 no_std 环境
- 描述符统一格式

### 2. HubPortOps Trait (端口操作)

```rust
pub trait HubPortOps: Send + Sync {
    // 基础信息
    fn port_number(&self) -> u8;
    fn device_speed(&self) -> Option<DeviceSpeed>;
    fn is_high_speed(&self) -> bool;

    // 端口状态
    unsafe fn read_status(&self) -> Result<PortStatus, USBError>;

    // 端口控制
    async fn reset(&mut self) -> Result<(), USBError>;
    async fn set_enable(&mut self, enable: bool) -> Result<(), USBError>;
    async fn set_power(&mut self, power: bool) -> Result<(), USBError>;
    async fn set_suspend(&mut self, suspend: bool) -> Result<(), USBError>;

    // 状态变化处理
    unsafe fn clear_status_change(&mut self) -> Result<(), USBError>;
}
```

**设计要点：**
- 参照 USB 2.0 规范 11.24
- 端口控制异步操作（复位、电源等）
- 状态查询同步返回（读取寄存器/缓存）

### 3. RootHub Trait (Root Hub 特定)

```rust
pub trait RootHub: Hub {
    // Host Controller 访问
    fn host_controller(&self) -> &dyn HostControllerOps;
    fn host_controller_mut(&mut self) -> &mut dyn HostControllerOps;

    // 控制器生命周期
    async fn wait_for_running(&mut self) -> Result<(), USBError>;
    fn reset_all_ports(&mut self) -> Result<(), USBError>;

    // 中断控制
    fn enable_irq(&mut self) -> Result<(), USBError>;
    fn disable_irq(&mut self) -> Result<(), USBError>;
}
```

**与 Host Controller 的关系：**
```
┌────────────────────────────────────────────┐
│              Xhci (Host Controller)        │
│                                             │
│  ┌────────────────────────────────────┐   │
│  │  RootHub (逻辑层)                  │   │
│  │  • 管理 DeviceContextList          │   │
│  │  • 管理 Command Ring               │   │
│  │  • 管理端口                        │   │
│  │                                     │   │
│  │  ┌────────────────────────────┐   │   │
│  │  │  HostControllerOps (硬件)  │   │   │
│  │  │  • read_reg()              │   │   │
│  │  │  • write_reg()             │   │   │
│  │  │  • dma_map()               │   │   │
│  │  └────────────────────────────┘   │   │
│  └────────────────────────────────────┘   │
└────────────────────────────────────────────┘
```

### 4. ExternalHub Trait (External Hub 特定)

```rust
pub trait ExternalHub: Hub + Device {
    // 设备标识
    fn device_address(&self) -> u8;

    // 状态变化端点
    fn status_change_endpoint(&mut self) -> Result<Box<dyn EndpointInterruptIn>, USBError>;

    // Hub 类请求
    fn hub_control(
        &mut self,
        request: HubRequest,
        value: u16,
        index: u16,
        data: &mut [u8],
    ) -> LocalBoxFuture<'_, Result<usize, USBError>>;

    // Transaction Translator
    fn tt_info(&self) -> Option<TtInfo>;
    fn needs_tt(&self) -> bool;
}
```

**USB 协议层封装：**
```
┌────────────────────────────────────────────┐
│          ExternalHub (USB Device)          │
│                                             │
│  实现 Device trait:                         │
│  • control_in()  → 发送 Hub 类请求         │
│  • control_out() → 发送 Hub 类请求         │
│                                             │
│  实现 Hub trait:                            │
│  • hub_control() → 封装标准 Hub 请求       │
│    - GET_HUB_DESCRIPTOR                    │
│    - GET_PORT_STATUS                       │
│    - SET_PORT_FEATURE (RESET, ENABLE...)   │
│                                             │
│  状态变化:                                   │
│  • EndpointInterruptIn 接收变化通知        │
└────────────────────────────────────────────┘
```

## Linux 对应关系

| CrabUSB | Linux | 说明 |
|---------|-------|------|
| `Hub` | `struct usb_hub` | Hub 设备抽象 |
| `HubPortOps` | `hub_port_*` 函数 | 端口操作 |
| `PortStatus` | `struct usb_port_status` | 端口状态结构 |
| `RootHub` | `struct usb_hcd` | Host Controller + rh_dev |
| `ExternalHub` | `drivers/usb/core/hub.c` | 外部 Hub 驱动 |
| `HubDescriptor` | `struct usb_hub_descriptor` | Hub 描述符 |
| `TtInfo` | `struct usb_tt` | Transaction Translator |

## 实现示例

### Root Hub 实现

```rust
pub struct XhciRootHub {
    reg: XhciRegisters,
    num_ports: u8,
    // ...
}

impl Hub for XhciRootHub {
    fn num_ports(&self) -> u8 {
        // 从 HCSPARAMS1 寄存器读取
        unsafe {
            let hcsparams1 = self.reg.capability.hcsparams1.read_volatile();
            ((hcsparams1 >> 24) & 0xFF) as u8
        }
    }

    fn port(&mut self, port_index: u8) -> Result<Box<dyn HubPortOps>, USBError> {
        Ok(Box::new(XhciPort::new(
            self.reg.mmio_base,
            port_index,
        )))
    }
}

impl RootHub for XhciRootHub {
    fn host_controller(&self) -> &dyn HostControllerOps {
        self
    }

    async fn wait_for_running(&mut self) -> Result<(), USBError> {
        // 等待 USBSTS.HCHalted = 0
        while unsafe { self.reg.operational.usbsts.read_volatile() } & 0x1000 != 0 {
            core::hint::spin_loop();
        }
        Ok(())
    }
}
```

### External Hub 实现

```rust
pub struct ExternalHubDevice {
    device: Box<dyn Device>,
    num_ports: u8,
    descriptor: HubDescriptor,
    tt: Option<TtInfo>,
}

impl Hub for ExternalHubDevice {
    fn hub_descriptor(&self) -> LocalBoxFuture<'_, Result<HubDescriptor, USBError>> {
        async { Ok(self.descriptor.clone()) }.boxed_local()
    }

    fn port(&mut self, port_index: u8) -> Result<Box<dyn HubPortOps>, USBError> {
        Ok(Box::new(ExternalHubPort::new(
            self.device.as_mut(),
            port_index,
        )))
    }
}

impl ExternalHub for ExternalHubDevice {
    fn device_address(&self) -> u8 {
        // 从 Device trait 获取
        todo!()
    }

    fn hub_control(
        &mut self,
        request: HubRequest,
        value: u16,
        index: u16,
        data: &mut [u8],
    ) -> LocalBoxFuture<'_, Result<usize, USBError>> {
        // 转换为 USB Control 传输
        let setup = ControlSetup {
            request_type: RequestType::Class,
            recipient: Recipient::Device,
            request: match request {
                HubRequest::GetHubDescriptor => Request(0x06),
                // ...
            },
            value,
            index,
            length: data.len() as u16,
        };

        self.device.control_out(setup, &[]).await?;
        // ...
        async { Ok(0) }.boxed_local()
    }

    fn tt_info(&self) -> Option<TtInfo> {
        self.tt
    }
}
```

## 使用场景

### 设备枚举流程

```rust
async fn enumerate_devices(hub: &mut dyn Hub) -> Result<Vec<DeviceInfo>, USBError> {
    let mut devices = Vec::new();

    for port_idx in 0..hub.num_ports() {
        let port = hub.port(port_idx)?;

        // 检查连接状态
        let status = unsafe { port.read_status()? };
        if !status.connected {
            continue;
        }

        // 复位端口
        port.reset().await?;

        // 获取设备速度
        let speed = port.device_speed().unwrap();

        // 分配地址并读取描述符
        // ...
    }

    Ok(devices)
}
```

### 端口状态监控

```rust
async fn monitor_port_changes(hub: &mut dyn Hub) {
    loop {
        unsafe {
            hub.handle_event().await.unwrap();
        }

        let statuses = hub.port_status_all().await.unwrap();
        for (port_idx, status) in statuses.iter().enumerate() {
            if status.change.connection_changed {
                log::info!("Port {} connection changed", port_idx);

                let port = hub.port(port_idx as u8).unwrap();
                unsafe {
                    port.clear_status_change().unwrap();
                }
            }
        }
    }
}
```

## 设计优势

### 1. 统一抽象
- Root Hub 和 External Hub 使用相同的接口
- 设备枚举代码可以复用
- 简化上层逻辑

### 2. 类型安全
- 编译时保证接口完整性
- 明确的 Trait 约束
- 防止误用

### 3. 异步友好
- 所有操作都是 Future
- 支持 no_std 环境
- 可与不同执行器集成

### 4. 贴近规范
- 严格遵循 USB 2.0/3.x 规范
- 与 Linux 驱动对应
- 易于理解和维护

## 参考资料

- USB 2.0 Specification, Section 11 (Hub)
- USB 3.0 Specification, Section 10 (Hub)
- Linux Kernel: `drivers/usb/core/hub.c`
- Linux Kernel: `include/linux/usb/hcd.h`
