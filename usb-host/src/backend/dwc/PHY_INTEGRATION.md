# RK3588 USBDP PHY 驱动使用指南

## 概述

`phy.rs` 实现了 RK3588 SoC 的 USBDP (USB3.0 + DisplayPort) Combo PHY 驱动。该 PHY 支持：
- USB3.0 SuperSpeed (5Gbps)
- USB2.0 HS/FS/LS
- DisplayPort 1.4 Alt Mode
- Lane multiplexing (USB/DP 通道共享)

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                    DWC3 Controller                         │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────┐   │
│  │   xHCI      │  │  DWC3 GLB    │  │   PHY Config     │   │
│  │   Host      │  │   Registers  │  │   Registers      │   │
│  └──────┬──────┘  └──────┬───────┘  └────────┬─────────┘   │
│         │                │                   │              │
└─────────┼────────────────┼───────────────────┼──────────────┘
          │                │                   │
          │            USB3 PHY              USB2 PHY
          │                │                   │
┌─────────┼────────────────┼───────────────────┼──────────────┐
│         │        ┌───────▼───────┐   ┌───────▼───────┐     │
│         │        │   USBDP PHY   │   │  USB2 PHY     │     │
│         │        │   (Combo)     │   │  (Separate)   │     │
│         │        │               │   │               │     │
│         │        │  • PMA        │   │  • UTMI       │     │
│         │        │  • PCS        │   │  • 480MHz     │     │
│         │        │  • Lane Mux   │   │               │     │
│         │        └───────┬───────┘   └───────┬───────┘     │
│         │                │                   │              │
│         │        ┌───────▼───────────────────▼───────┐    │
│         │        │        GRF Registers            │    │
│         │        │  • usbdpphy-grf                  │    │
│         │        │  • u2phy-grf                     │    │
│         │        │  • usb-grf                       │    │
│         │        └──────────────────────────────────┘    │
└─────────┼─────────────────────────────────────────────────┘
          │
    ┌─────▼─────┐
    │   CRU     │  Clock & Reset
    └───────────┘
```

## 基本使用

### 1. 仅 USB3.0 模式

```rust
use crab_usb::backend::dwc::{UsbDpPhyConfig, UsbDpMode, init_rk3588_usb_phy};

// 创建 PHY 配置
let phy_config = UsbDpPhyConfig {
    id: 0,                          // PHY0
    mode: UsbDpMode::Usb,           // 仅 USB 模式
    flip: false,
    dp_lane_map: [0, 1, 2, 3],
};

// 初始化 PHY
init_rk3588_usb_phy(&mut dwc3_regs, phy_config)?;
```

### 2. USB + DisplayPort Combo 模式

```rust
let phy_config = UsbDpPhyConfig {
    id: 0,
    mode: UsbDpMode::UsbDp,         // Combo 模式
    flip: false,
    dp_lane_map: [2, 3, 0, 1],     // Lane 2,3 用于 DP
};

init_rk3588_usb_phy(&mut dwc3_regs, phy_config)?;
```

### 3. 仅 DisplayPort 模式

```rust
let phy_config = UsbDpPhyConfig {
    id: 0,
    mode: UsbDpMode::Dp,            // 仅 DP 模式
    flip: false,
    dp_lane_map: [0, 1, 2, 3],     // 所有 lane 用于 DP
};

init_rk3588_usb_phy(&mut dwc3_regs, phy_config)?;
```

## 完整初始化流程

```rust
use crab_usb::backend::dwc::{Dwc, UsbDpPhyConfig, UsbDpMode};

async fn init_usb_controller(mmio_base: usize) -> Result<()> {
    // 1. 创建 DWC3 控制器
    let mut dwc = Dwc::new(
        Mmio::from(mmio_base),
        0xffffffff, // DMA mask
    )?;

    // 2. 配置 PHY
    let phy_config = UsbDpPhyConfig {
        id: 0,
        mode: UsbDpMode::Usb,
        ..Default::default()
    };

    // 3. 初始化 PHY (在 DWC init 之前)
    init_rk3588_usb_phy(&mut dwc.dwc_regs, phy_config)?;

    // 4. 初始化 DWC3 控制器
    dwc.init().await?;

    Ok(())
}
```

## 寄存器地址

### PHY 寄存器

| 组件 | 基地址 | 描述 |
|------|--------|------|
| USBDP PHY0 | 0xfed80000 | USB3+DP Combo PHY |
| USBDP PHY1 | 0xfed90000 | USB3+DP Combo PHY |
| PMA | Base + 0x8000 | 物理媒质附加层 |
| PCS | Base + 0x4000 | 物理编码子层 |

### GRF 寄存器

| GRF | 基地址 | 描述 |
|-----|--------|------|
| usbdpphy-grf | 0xfd5c8000 | USBDP PHY 通用寄存器 |
| u2phy-grf | 0xfd5d0000 | USB2 PHY 通用寄存器 |
| usb-grf | 0xfd5ac000 | USB 通用寄存器 |
| vo-grf | 0xfd5f0000 | 视频输出通用寄存器 |

### 时钟和复位

| ID | 名称 | 描述 |
|----|------|------|
| CLK 694 (0x2b6) | refclk | USBDP PHY 参考时钟 |
| CLK 639 (0x27f) | immortal | USBDP PHY 不朽时钟 |
| CLK 617 (0x269) | pclk | USBDP PHY APB 时钟 |
| RST 40 (0x28) | init | 初始化复位 |
| RST 41 (0x29) | cmn | 公共复位 |
| RST 42 (0x2a) | lane | 通道复位 |
| RST 43 (0x2b) | pcs_apb | PCS APB 复位 |
| RST 1154 (0x482) | pma_apb | PMA APB 复位 |

## Lane 映射

### Type-C Mapping Table

| Type-C Pin | PHY Pad | C/E Normal | C/E Flip | D/F Normal | D/F Flip |
|------------|---------|------------|----------|------------|----------|
| B11-B10    | ln0     | dpln3      | dpln0    | usbrx      | dpln0    |
| A2-A3      | ln1     | dpln2      | dpln1    | usbtx      | dpln1    |
| A11-A10    | ln2     | dpln0      | dpln3    | dpln0      | usbrx    |
| B2-B3      | ln3     | dpln1      | dpln2    | dpln1      | usbtx    |

### 模式和 Lane 使用

| 模式 | Lane 0 | Lane 1 | Lane 2 | Lane 3 | 描述 |
|------|--------|--------|--------|--------|------|
| USB | USB | USB | USB | USB | 全部 4 个 lane 用于 USB3 |
| DP | DP | DP | DP | DP | 全部 4 个 lane 用于 DP |
| Combo | USB | USB | DP | DP | Lane 0,1 USB; Lane 2,3 DP |

## 时钟配置

### 必须启用的时钟

1. **refclk** (694/0x2b6): 24MHz 参考时钟
   ```c
   clk_enable(CLK_USBDP_PHY_REFCLK);
   ```

2. **immortal** (639/0x27f): 常开时钟
   ```c
   clk_enable(CLK_USBDP_PHY_IMMORTAL);
   ```

3. **pclk** (617/0x269): APB 总线时钟
   ```c
   clk_enable(CLK_USBDP_PHY_PCLK);
   ```

### 时序要求

```
refclk ──────────────────────────────────────>
          ↓
        24MHz
          ↓
    ┌─────────┐
    │  PLL    │
    │  Lock   │  (~100ms)
    └────┬────┘
         ↓
    PHY Ready
```

## 复位序列

### 标准复位序列

```
1. Assert 所有复位
   ├─ init (40)
   ├─ cmn (41)
   ├─ lane (42)
   ├─ pcs_apb (43)
   └─ pma_apb (1154)

2. 等待 10ms

3. Deassert APB 复位
   ├─ pcs_apb (43)
   └─ pma_apb (1154)

4. 配置寄存器

5. Deassert PHY 复位
   ├─ init (40)
   ├─ cmn (41)
   └─ lane (42)

6. 等待 PLL 锁定 (100ms 超时)
   └─ 检查 LCPLL_LOCK_DONE
   └─ 检查 ROPLL_LOCK_DONE
```

## GRF 配置

### usbdpphy-grf[0x0004]

| Bit | 字段 | 值 | 描述 |
|-----|------|----|----|
| 14 | RX_LFPS | 1 | 启用 RX LFPS (USB 模式) |
| 13 | LOW_PWRN | 1 | 退出低功耗模式 |

### usb-grf[0x001c] (USB3OTG0)

| Bit | 字段 | 值 | 描述 |
|-----|------|----|----|
| 15:0 | 配置 | 0x0188 | 启用 USB3 端口 |
| 15:0 | 配置 | 0x1100 | 禁用 USB3 端口 |

## 调试

### 启用详细日志

```rust
// 设置环境变量
export RUST_LOG=crab_usb::backend::dwc=debug

// 或在代码中
log::set_max_level(log::LevelFilter::Debug);
```

### 检查 PHY 状态

```rust
let phy = unsafe { UsbDpPhy::new(config) };
let status = phy.get_status();

println!("LCPLL Locked: {}", status.lcpll_locked);
println!("ROPLL Locked: {}", status.ropll_locked);
println!("Mode: {:?}", status.mode);
```

### 常见问题

#### 1. PHY 未锁定

```
⚠ LCPLL not locked, USB3 may not work
```

**可能原因**:
- 时钟未启用
- 复位序列错误
- 参考时钟频率不正确

**解决方案**:
```rust
// 检查时钟
verify_clock_enabled(CLK_USBDP_PHY_REFCLK);
verify_clock_enabled(CLK_USBDP_PHY_IMMORTAL);
verify_clock_enabled(CLK_USBDP_PHY_PCLK);

// 检查复位
verify_reset_deasserted(RST_USBDP_INIT);
verify_reset_deasserted(RST_USBDP_CMN);
verify_reset_deasserted(RST_USBDP_LANE);
```

#### 2. 设备未检测到

**可能原因**:
- USB3 端口未启用
- PHY 模式配置错误
- Lane 映射不正确

**解决方案**:
```rust
// 确保启用 U3 端口
phy.enable_u3_port();

// 检查模式配置
assert_eq!(config.mode, UsbDpMode::Usb);
```

## 参考

- Linux: `drivers/phy/rockchip/phy-rockchip-usbdp.c`
- U-Boot: `drivers/phy/phy-rockchip-usbdp.c`
- RK3588 TRM: Chapter on USBDP PHY
- USB 3.0 Specification
- DisplayPort Alt Mode Specification
