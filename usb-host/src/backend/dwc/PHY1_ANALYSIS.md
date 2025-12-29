# RK3588 USBDP PHY1 寄存器布局分析

## 设备树配置

**PHY1 @ 0xfed90000** (来自 Orange Pi 5 Plus 设备树)

```dts
phy@fed90000 {
    compatible = "rockchip,rk3588-usbdp-phy";
    reg = <0x00 0xfed90000 0x00 0x10000>;

    // GRF 引用
    rockchip,u2phy-grf = <0x1cc>;      // USB2 PHY GRF
    rockchip,usb-grf = <0x75>;         // USB GRF
    rockchip,usbdpphy-grf = <0x1cd>;   // USBDP PHY GRF
    rockchip,vo-grf = <0xfc>;          // Video Output GRF

    // 时钟配置
    clocks = <0x02 0x2b6   // refclk:  694 (0x2b6)
             0x02 0x280   // immortal: 640 (0x280)
             0x02 0x26a   // pclk:     618 (0x26a)
             0x1ce>;      // utmi:     USB2 PHY1 输出
    clock-names = "refclk\0immortal\0pclk\0utmi";

    // 复位配置
    resets = <0x02 0x2f   // init:     47  (0x2f)
             0x02 0x30   // cmn:      48  (0x30)
             0x02 0x31   // lane:     49  (0x31)
             0x02 0x32   // pcs_apb:  50  (0x32)
             0x02 0x484>; // pma_apb:  1156 (0x484)
    reset-names = "init\0cmn\0lane\0pcs_apb\0pma_apb";

    // DP lane 映射
    rockchip,dp-lane-mux = <0x02 0x03>;  // Lane 2,3 用于 DP

    status = "okay";

    // 子端口
    dp-port { ... };
    u3-port { ... };
};
```

## 寄存器地址映射

### 整体布局

```
USBDP PHY1 @ 0xfed90000 (64KB 空间)
│
├─ 0x0000 - 0x3FFF: PCS (Physical Coding Sublayer)
│  └─ Base + 0x4000
│     ├─ PCS_RESET_CONTROL
│     ├─ PCS_POWER_DOWN_CONTROL
│     └─ PCS_LANE_CONTROL[0-3]
│
├─ 0x4000 - 0x7FFF: Reserved
│
└─ 0x8000 - 0xFFFF: PMA (Physical Medium Attachment)
   └─ Base + 0x8000
      ├─ CMN (Common) Registers
      │  ├─ 0x8288: CMN_LANE_MUX_AND_EN
      │  ├─ 0x828C: CMN_DP_LINK
      │  ├─ 0x8350: CMN_ANA_LCPLL_DONE
      │  ├─ 0x8354: CMN_ANA_ROPLL_DONE
      │  └─ 0x838C: CMN_DP_RSTN
      │
      └─ TRSV (Transceiver) Registers
         ├─ Lane 0 @ 0x8800
         ├─ Lane 1 @ 0x9000
         ├─ Lane 2 @ 0x9800
         │  └─ 0x9B84: TRSV_LN2_MON_RX_CDR_DONE
         └─ Lane 3 @ 0xA000
```

## 关键时钟配置

### 时钟 ID 解码

| 时钟名称 | ID (十进制) | ID (十六进制) | CRU 常量 | 频率 | 来源 |
|---------|-----------|-------------|---------|------|------|
| **refclk** | 694 | 0x2b6 | CLK_USBDPPHY_MIPIDCPPHY_REF | 24MHz | CRU |
| **immortal** | 640 | 0x280 | CLK_USBDP_PHY1_IMMORTAL | Always-on | CRU |
| **pclk** | 618 | 0x26a | PCLK_USBDPPHY1 | APB | CRU |
| **utmi** | - | 0x1ce | USB2 PHY1 output | 480MHz | USB2 PHY1 |

### CRU 时钟定义

```c
// u-boot-orangepi/include/dt-bindings/clock/rk3588-cru.h
#define PCLK_USBDPPHY1              618  // 0x26a
#define CLK_USBDP_PHY1_IMMORTAL      640  // 0x280
#define CLK_USBDPPHY_MIPIDCPPHY_REF  694  // 0x2b6
```

### 时钟树

```
CRU (Clock & Reset Unit)
├─ OSC: 24MHz
│  ├─> refclk (694) ─────────────> USBDP PHY1 (refclk)
│  │
│  └─> GPLL (1188MHz)
│     └─> immortal (640) ────────> USBDP PHY1 (immortal)
│
└─ ACLK_BUS (400MHz)
   └─> pclk (618) ──────────────> USBDP PHY1 (APB)

USB2 PHY1
   └─> utmi (480MHz) ────────────> USBDP PHY1 (UTMI)
```

## 复位配置

### 复位 ID 解码

| 复位名称 | ID (十进制) | ID (十六进制) | CRU 常量 | 描述 |
|---------|-----------|-------------|---------|------|
| **init** | 47 | 0x2f | SRST_USBDP_COMBO_PHY1_INIT | 初始化复位 |
| **cmn** | 48 | 0x30 | SRST_USBDP_COMBO_PHY1_CMN | 公共部分复位 |
| **lane** | 49 | 0x31 | SRST_USBDP_COMBO_PHY1_LANE | Lane 复位 |
| **pcs_apb** | 50 | 0x32 | SRST_USBDP_COMBO_PHY1_PCS | PCS APB 复位 |
| **pma_apb** | 1156 | 0x484 | SRST_P_USBDPPHY1 | PMA APB 复位 |

### CRU 复位定义

```c
// u-boot-orangepi/include/dt-bindings/clock/rk3588-cru.h
#define SRST_USBDP_COMBO_PHY1_INIT    47   // 0x2f
#define SRST_USBDP_COMBO_PHY1_CMN     48   // 0x30
#define SRST_USBDP_COMBO_PHY1_LANE    49   // 0x31
#define SRST_USBDP_COMBO_PHY1_PCS     50   // 0x32
#define SRST_P_USBDPPHY1             1156   // 0x484
```

### 复位序列

```
1. Assert 所有复位
   ├─ init (47)
   ├─ cmn (48)
   ├─ lane (49)
   ├─ pcs_apb (50)
   └─ pma_apb (1156)

2. 等待 10ms

3. Deassert APB 复位 (使能寄存器访问)
   ├─ pcs_apb (50)
   └─ pma_apb (1156)

4. 配置 PHY 寄存器

5. Deassert PHY 复位
   ├─ init (47)
   ├─ cmn (48)
   └─ lane (49)

6. 等待 PLL 锁定 (100ms)
```

## DP Lane Mux 配置

### 设备树配置

```dts
rockchip,dp-lane-mux = <0x02 0x03>;
```

### 解析

| 值 | Lane | 用途 |
|----|------|------|
| 0x02 | Lane 2 | DP (DisplayPort) |
| 0x03 | Lane 3 | DP (DisplayPort) |
| - | Lane 0 | USB (默认) |
| - | Lane 1 | USB (默认) |

### Lane Mux 寄存器值

```c
// CMN_LANE_MUX_AND_EN @ 0xfed98000 + 0x0288 = 0xfed98288
// 值 = 0b11111100 = 0xFC
//       └─┘└┘└┘└─┘
//         L3 L2 L1 L0
//         DP DP USB USB

#define CMN_LANE_MUX_AND_EN  0xFC
```

**寄存器位字段**：
- Bits [7:4]: Lane Mux
  - [4] = 0 (Lane 0 = USB)
  - [5] = 0 (Lane 1 = USB)
  - [6] = 1 (Lane 2 = DP)
  - [7] = 1 (Lane 3 = DP)
- Bits [3:0]: Lane Enable
  - [0] = 1 (Enable Lane 0)
  - [1] = 1 (Enable Lane 1)
  - [2] = 1 (Enable Lane 2)
  - [3] = 1 (Enable Lane 3)

## GRF 引用解析

### GRF Phandle 查找

| Phandle | GRF 类型 | 基地址 | 描述 |
|---------|---------|--------|------|
| **0x1cc** | u2phy-grf | 0xfd5d4000 | USB2 PHY1 GRF |
| **0x75** | usb-grf | 0xfd5ac000 | USB GRF (全局) |
| **0x1cd** | usbdpphy-grf | 0xfd5cc000 | USBDP PHY1 GRF |
| **0xfc** | vo-grf | 0xfd5f0000 | Video Output GRF |

### GRF 寄存器

#### USBDP PHY1 GRF @ 0xfd5cc000

```c
// 低功耗控制寄存器
#define USBDPPHY_LOW_PWRN    0x0004

// 位字段
#define RX_LFPS_EN           BIT(14)  // RX LFPS enable (USB mode)
#define LOW_PWRN             BIT(13)  // Low power mode (1=power up)
```

#### USB GRF @ 0xfd5ac000

```c
// USB3 OTG1 配置寄存器
#define USB3OTG1_CFG         0x0034

// 配置值
#define USB3_ENABLED         0x0188   // 启用 USB3
#define USB3_DISABLED        0x1100   // 禁用 USB3
```

## 寄存器访问示例

### 初始化 USBDP PHY1

```rust
use crab_usb::backend::dwc::{UsbDpPhy, UsbDpPhyConfig, UsbDpMode};

// PHY1 配置
let config = UsbDpPhyConfig {
    id: 1,  // PHY1
    mode: UsbDpMode::UsbDp,  // USB + DP Combo
    flip: false,
    dp_lane_map: [2, 3, 0, 1],  // Lane 2,3 用于 DP
};

// 创建 PHY 驱动
let mut phy = unsafe { UsbDpPhy::new(config) };

// 初始化
phy.init()?;

// 启用 U3 端口
phy.enable_u3_port();
```

### 读取 PLL 状态

```rust
let pma_base = 0xfed90000 + 0x8000;

// 读取 LCPLL 状态 (Lane 2,3 使用)
let lcpll = unsafe { ((pma_base + 0x0350) as *const u32).read_volatile() };
let lcpll_locked = (lcpll >> 7) & 0x1 == 1;

// 读取 ROPLL 状态 (DP 使用)
let ropll = unsafe { ((pma_base + 0x0354) as *const u32).read_volatile() };
let ropll_locked = (ropll >> 1) & 0x1 == 1;

println!("PHY1: LCPLL={}, ROPLL={}", lcpll_locked, ropll_locked);
```

### 配置 Lane Mux

```rust
// 设置 Lane 2,3 为 DP，Lane 0,1 为 USB
let pma_base = 0xfed90000 + 0x8000;
let lane_mux_reg = (pma_base + 0x0288) as *mut u32;

// 值 = 0xFC (参考设备树 dp-lane-mux = <0x02 0x03>)
unsafe {
    lane_mux_reg.write_volatile(0xFC);
}
```

## PHY0 vs PHY1 对比

| 特性 | PHY0 (0xfed80000) | PHY1 (0xfed90000) |
|------|------------------|------------------|
| **refclk ID** | 694 (0x2b6) | 694 (0x2b6) |
| **immortal ID** | 639 (0x27f) | 640 (0x280) |
| **pclk ID** | 617 (0x269) | 618 (0x26a) |
| **init rst** | 40 (0x28) | 47 (0x2f) |
| **cmn rst** | 41 (0x29) | 48 (0x30) |
| **lane rst** | 42 (0x2a) | 49 (0x31) |
| **pcs_apb rst** | 43 (0x2b) | 50 (0x32) |
| **pma_apb rst** | 1154 (0x482) | 1156 (0x484) |
| **u2phy-grf** | 0xfd5d0000 | 0xfd5d4000 |
| **usbdpphy-grf** | 0xfd5c8000 | 0xfd5cc000 |
| **dp-lane-mux** | varies | <0x02 0x03> |

## 使用场景

### 场景 1: USB 3.0 Only

```rust
let config = UsbDpPhyConfig {
    id: 1,
    mode: UsbDpMode::Usb,  // 仅 USB
    ..Default::default()
};
```

### 场景 2: USB + DP Combo (Lane 2,3 用于 DP)

```rust
let config = UsbDpPhyConfig {
    id: 1,
    mode: UsbDpMode::UsbDp,
    dp_lane_map: [2, 3, 0, 1],  // Lane 2,3 = DP
    ..Default::default()
};
```

### 场景 3: DisplayPort Only

```rust
let config = UsbDpPhyConfig {
    id: 1,
    mode: UsbDpMode::Dp,
    dp_lane_map: [0, 1, 2, 3],  // 全部 DP
    ..Default::default()
};
```

## 相关文档

- Linux: `drivers/phy/rockchip/phy-rockchip-usbdp.c`
- U-Boot: `drivers/phy/phy-rockchip-usbdp.c`
- RK3588 TRM: USBDP PHY 章节
- `rk3588-clk/platform/orangepi.dts`: 设备树源文件
- `include/dt-bindings/clock/rk3588-cru.h`: 时钟定义
