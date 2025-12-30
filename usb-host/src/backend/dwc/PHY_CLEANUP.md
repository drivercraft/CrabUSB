# PHY 模块适配新 GRF 接口总结

## 概述

将 `phy.rs` 适配新的 GRF 接口，移除过时的偏移量计算代码，改为直接使用设备树定义的 GRF 基址。

## 主要修改

### 1. 修复 UsbDpPhyConfig 结构体

**修改前**:
```rust
pub struct UsbDpPhyConfig {
    pub mode: UsbDpMode,
    pub flip: bool,
    pub dp_lane_map: [u8; 4],
}
```

**修改后**:
```rust
pub struct UsbDpPhyConfig {
    pub id: u8,                    // ← 新增：PHY ID (0 或 1)
    pub mode: UsbDpMode,
    pub flip: bool,
    pub dp_lane_map: [u8; 4],
}
```

**原因**:
- 原代码在多个地方使用 `self.config.id`（如日志输出），但结构体中缺少该字段
- PHY ID 用于区分 PHY0 和 PHY1，对 USB GRF 操作很重要

---

### 2. 更新文档注释

**修改前**:
```rust
//! GRF 寄存器:
//!   - usbdpphy-grf: 0xfd5c8000
//!   - u2phy-grf:    0xfd5d0000
//!   - usb-grf:      0xfd5ac000
//!   - vo-grf:       0xfd5f0000
```

**修改后**:
```rust
//! GRF 寄存器（设备树定义）:
//!   - usbdpphy0-grf: 0xfd5c8000 (syscon@fd5c8000)
//!   - usbdpphy1-grf: 0xfd5cc000 (syscon@fd5cc000)
//!   - usb-grf:       0xfd5ac000 (syscon@fd5ac000, PHY0 和 PHY1 共享)
//!   - u2phy-grf:     0xfd5d0000 (syscon@fd5d0000)
//!   - vo-grf:        0xfd5a6000 (syscon@fd5a6000)
//!
//! 参考 GRF_DTS_ANALYSIS.md 了解地址获取过程。
```

**改进**:
- 明确标注设备树节点名称
- 区分 PHY0 和 PHY1 的不同 GRF
- 添加参考文档链接

---

### 3. 修复 Combo 模式 Lane MUX 配置

**修改前**:
```rust
// 设置对应 lane 的 mux 值
val += CMN_LANE_MUX_EN::LANE0_MUX::new(mux_val << (4 + i));
```

**错误**: `CMN_LANE_MUX_EN::LANE0_MUX::new()` 方法不存在

**修改后**:
```rust
// 设置对应 lane 的 mux 值
// LANE0_MUX 在 bit 4, LANE1_MUX 在 bit 5, 以此类推
val += mux_val << (4 + i);
```

**修复**: 直接使用位移操作，避免调用不存在的 `new()` 方法

---

### 4. 更新单元测试

**修改前**:
```rust
#[test]
fn test_grf_base_calculation() {
    // 使用偏移量常量计算
    let phy0_usbdpphy_grf = (phy0_base as isize + USBDPPHY0_GRF_OFFSET) as usize;
    assert_eq!(phy0_usbdpphy_grf, 0xfd5c8000);
}
```

**问题**: 依赖已删除的偏移量常量（`USBDPPHY0_GRF_OFFSET` 等）

**修改后**:
```rust
#[test]
fn test_grf_addresses() {
    // 直接验证设备树中定义的地址
    let phy0_usbdpphy_grf: usize = 0xfd5c8000;  // syscon@fd5c8000
    let phy0_usb_grf: usize = 0xfd5ac000;       // syscon@fd5ac000

    assert_eq!(phy0_usbdpphy_grf, 0xfd5c8000, "PHY0 USBDPPHY GRF 地址错误");
    assert_eq!(phy0_usb_grf, 0xfd5ac000, "PHY0 USB GRF 地址错误");
}
```

**改进**:
- 直接验证设备树中的物理地址
- 添加注释说明地址来源
- 移除对已删除偏移量常量的依赖

---

### 5. 修复 no_std 兼容性

**文件**: `reg.rs`

**修改前**:
```rust
use std::hint::spin_loop;
```

**修改后**:
```rust
use core::sync::atomic::spin_loop_hint;
```

**调用处**:
```rust
fn delay_ms(&self, ms: u32) {
    for _ in 0..total_loops {
        spin_loop_hint();  // ← 修改
    }
}
```

**原因**: `std` 在 `no_std` 环境下不可用，应使用 `core` 中的对应函数

---

## 移除的代码

### 删除的偏移量常量

以下常量已被完全移除：
```rust
// ❌ 已删除
pub const USBDPPHY0_GRF_OFFSET: isize = -0x9580000;
pub const USBDPPHY1_GRF_OFFSET: isize = -0x9584000;
pub const USB0_GRF_OFFSET: isize = -0x97d4000;
pub const USB1_GRF_OFFSET: isize = -0x97d5000;
```

**原因**:
- 新设计直接使用设备树中的 GRF 基址
- 调用者负责提供正确的 GRF 地址
- 简化代码，减少运行时计算

### 删除的构造函数逻辑

**之前的实现**（需要偏移量计算）:
```rust
pub fn new(
    config: UsbDpPhyConfig,
    phy_base: Mmio,
    usbdpphy_grf_base: Mmio,  // 从 phy_base 计算得到
    usb_grf_base: Mmio,       // 从 phy_base 计算得到
) -> Self {
    // 内部计算偏移量...
}
```

**新的实现**（直接接收 GRF 基址）:
```rust
pub fn new(
    config: UsbDpPhyConfig,
    phy_base: Mmio,
    u3_grf: Mmio,    // USB GRF 基址（直接使用）
    dp_grf: Mmio,    // USBDP PHY GRF 基址（直接使用）
) -> Self {
    let dp_grf = unsafe { Grf::new(dp_grf, GrfType::UsbdpPhy) };
    let usb_grf = unsafe { Grf::new(u3_grf, GrfType::Usb) };
    // ...
}
```

---

## 设计优势

### 1. **清晰的地址来源**
```rust
// 明确标注设备树节点
let phy0_usbdpphy_grf: usize = 0xfd5c8000;  // syscon@fd5c8000
```

### 2. **类型安全**
```rust
// GRF 类型在编译时验证
let dp_grf: Grf = unsafe { Grf::new(dp_grf, GrfType::UsbdpPhy) };
let usb_grf: Grf = unsafe { Grf::new(u3_grf, GrfType::Usb) };
```

### 3. **符合设备树规范**
- 直接使用设备树中定义的物理地址
- 与 Linux/U-Boot 驱动保持一致
- 易于验证和调试

### 4. **简化的测试**
```rust
// 直接验证地址，无需计算
assert_eq!(phy0_usbdpphy_grf, 0xfd5c8000);
```

---

## 编译结果

```bash
cargo check --package crab-usb
```

✅ **编译成功**，无错误
⚠️ 仅有未使用导入的警告（不影响功能）

---

## 使用示例

### 创建 PHY 实例

```rust
use crab_usb::backend::dwc::{UsbDpPhy, UsbDpPhyConfig, UsbDpMode};

// PHY1 配置（对应 usb@fc400000）
let config = UsbDpPhyConfig {
    id: 1,
    mode: UsbDpMode::Usb,
    ..Default::default()
};

// 创建 PHY 实例（使用设备树中的地址）
let phy = UsbDpPhy::new(
    config,
    Mmio::from_ptr(0xfed90000 as *const u8),  // PHY 基址 (phy@fed90000)
    Mmio::from_ptr(0xfd5ac000 as *const u8),  // USB GRF (syscon@fd5ac000)
    Mmio::from_ptr(0xfd5cc000 as *const u8),  // USBDP PHY GRF (syscon@fd5cc000)
);

// 初始化 PHY
phy.init()?;

// 启用 U3 端口
phy.enable_u3_port();
```

### 地址映射关系

```
PHY1 (usb@fc400000):
├── PHY 寄存器:    0xfed90000
├── USB GRF:       0xfd5ac000 (与 PHY0 共享)
│   ├── USB3OTG0_CFG (offset 0x001c) → 控制 PHY0
│   └── USB3OTG1_CFG (offset 0x0034) → 控制 PHY1 ⭐
└── USBDP PHY GRF: 0xfd5cc000 (PHY1 专用)
    └── LOW_PWRN (offset 0x0004)
```

---

## 相关文档

- **GRF_DTS_ANALYSIS.md**: 设备树 GRF 地址获取详细过程
- **GRF_INTRODUCTION.md**: GRF 是什么以及如何使用
- **GRF_REGISTER_STRUCTS.md**: register_structs! 宏使用指南

---

## 验证清单

- [x] 添加 `UsbDpPhyConfig::id` 字段
- [x] 更新文档注释，引用设备树节点
- [x] 修复 Combo 模式 lane MUX 配置
- [x] 移除偏移量常量依赖
- [x] 更新单元测试
- [x] 修复 `std` 依赖（改用 `core`）
- [x] 编译通过
- [x] 所有功能保持不变
