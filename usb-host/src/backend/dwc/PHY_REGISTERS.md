# RK3588 USBDP PHY 寄存器布局

## 地址映射

### 整体映射

```
USBDP PHY0 @ 0xfed80000
├── 0x0000 - 0x3FFF: PCS (Physical Coding Sublayer) 寄存器
├── 0x4000 - 0x7FFF: 保留区域
├── 0x8000 - 0xFFFF: PMA (Physical Medium Attachment) 寄存器
└── 0x10000 - ...:   保留
```

### PHY 基地址

| PHY | 基地址 | 描述 |
|-----|--------|------|
| USBDP PHY0 | 0xfed80000 | USB3+DP Combo PHY 0 |
| USBDP PHY1 | 0xfed90000 | USB3+DP Combo PHY 1 |

## PMA 寄存器布局 (Base + 0x8000)

### CMN (公共) 寄存器

#### Lane Mux 和 Enable

```c
// Offset: 0x8000 + 0x0288 = 0x8288
#define CMN_LANE_MUX_AND_EN_OFFSET    0x0288

// 位字段定义
#define CMN_DP_LANE_MUX_N(n)          BIT((n) + 4)  // Lane n multiplexer
#define CMN_DP_LANE_EN_N(n)           BIT(n)         // Lane n enable

// Lane mux 值
#define PHY_LANE_MUX_USB              0
#define PHY_LANE_MUX_DP               1

// 示例：设置 Lane 0 为 USB，Lane 2 为 DP
// reg = (PHY_LANE_MUX_USB << 4) | PHY_LANE_MUX_DP << 6 |
//        BIT(0) | BIT(2)
```

**寄存器详解**：

| Bits | 字段 | 描述 |
|------|------|------|
| [7:4] | LANE_MUX[3:0] | Lane 3-0 multiplexer (0=USB, 1=DP) |
| [3:0] | LANE_EN[3:0] | Lane 3-0 enable (1=enable) |

#### DP Link 配置

```c
// Offset: 0x8000 + 0x028c = 0x828c
#define CMN_DP_LINK_OFFSET            0x028c

// 位字段
#define CMN_DP_TX_LINK_BW             GENMASK(6, 5)  // 链路带宽
#define CMN_DP_TX_LANE_SWAP_EN        BIT(2)        // Lane swap enable

// 带宽值
#define DP_BW_RBR                     0              // 1.62 Gbps (RBR)
#define DP_BW_HBR                     1              // 2.70 Gbps (HBR)
#define DP_BW_HBR2                    2              // 5.40 Gbps (HBR2)
#define DP_BW_HBR3                    3              // 8.10 Gbps (HBR3)
```

**寄存器详解**：

| Bits | 字段 | 描述 |
|------|------|------|
| [6:5] | TX_LINK_BW | DP link bandwidth |
| [2] | LANE_SWAP_EN | Lane swap enable |

#### SSC (Spread Spectrum Clock) 使能

```c
// Offset: 0x8000 + 0x02d0 = 0x82d0
#define CMN_SSC_EN_OFFSET              0x02d0

// 位字段
#define CMN_ROPLL_SSC_EN              BIT(1)  // ROPLL SSC enable
#define CMN_LCPLL_SSC_EN              BIT(0)  // LCPLL SSC enable
```

**寄存器详解**：

| Bits | 字段 | 描述 |
|------|------|------|
| [1] | ROPLL_SSC_EN | ROPLL spread spectrum enable |
| [0] | LCPLL_SSC_EN | LCPLL spread spectrum enable |

#### LCPLL 状态

```c
// Offset: 0x8000 + 0x0350 = 0x8350
#define CMN_ANA_LCPLL_DONE_OFFSET      0x0350

// 位字段
#define CMN_ANA_LCPLL_LOCK_DONE       BIT(7)  // LCPLL locked
#define CMN_ANA_LCPLL_AFC_DONE        BIT(6)  // LCPLL AFC done
```

**寄存器详解**：

| Bits | 字段 | 描述 |
|------|------|------|
| [7] | LOCK_DONE | LCPLL lock done (1=locked) |
| [6] | AFC_DONE | LCPLL AFC done (1=done) |

#### ROPLL 状态

```c
// Offset: 0x8000 + 0x0354 = 0x8354
#define CMN_ANA_ROPLL_DONE_OFFSET      0x0354

// 位字段
#define CMN_ANA_ROPLL_LOCK_DONE       BIT(1)  // ROPLL locked
#define CMN_ANA_ROPLL_AFC_DONE        BIT(0)  // ROPLL AFC done
```

**寄存器详解**：

| Bits | 字段 | 描述 |
|------|------|------|
| [1] | LOCK_DONE | ROPLL lock done (1=locked) |
| [0] | AFC_DONE | ROPLL AFC done (1=done) |

#### DP 复位控制

```c
// Offset: 0x8000 + 0x038c = 0x838c
#define CMN_DP_RSTN_OFFSET             0x038c

// 位字段
#define CMN_DP_INIT_RSTN              BIT(3)  // DP init reset
#define CMN_DP_CMN_RSTN               BIT(2)  // DP common reset
#define CMN_CDR_WTCHDG_EN             BIT(1)  // CDR watchdog enable
#define CMN_CDR_WTCHDG_MSK_CDR_EN     BIT(0)  // CDR watchdog mask
```

**寄存器详解**：

| Bits | 字段 | 描述 |
|------|------|------|
| [3] | INIT_RSTN | DP init reset (1=deassert) |
| [2] | CMN_RSTN | DP common reset (1=deassert) |
| [1] | WTCHDG_EN | CDR watchdog enable |
| [0] | MSK_CDR_EN | Mask CDR enable |

### TRSV (Transceiver) 寄存器

每个 Lane 有独立的 TRSV 寄存器区域，大小为 0x800。

#### Lane 0 CDR 状态

```c
// Offset: 0x8000 + 0x0b84 = 0x8b84
#define TRSV_LN0_MON_RX_CDR_DONE_OFFSET 0x0b84

// 位字段
#define TRSV_LN0_MON_RX_CDR_LOCK_DONE  BIT(0)  // Lane 0 CDR locked
```

#### Lane 2 CDR 状态

```c
// Offset: 0x8000 + 0x1b84 = 0x9b84
#define TRSV_LN2_MON_RX_CDR_DONE_OFFSET 0x1b84

// 位字段
#define TRSV_LN2_MON_RX_CDR_LOCK_DONE  BIT(0)  // Lane 2 CDR locked
```

#### TX 时钟配置

```c
// 基础偏移: 0x8000 + 0x854 = 0x8854
// Lane n 偏移: 0x8854 + n * 0x800

#define TRSV_ANA_TX_CLK_OFFSET_N(n)    (0x854 + (n) * 0x800)

// 位字段
#define LN_ANA_TX_SER_TXCLK_INV       BIT(1)  // TX clock invert
```

**寄存器地址映射**：

| Lane | Offset | 描述 |
|------|--------|------|
| 0 | 0x8854 | TX clock Lane 0 |
| 1 | 0x9054 | TX clock Lane 1 |
| 2 | 0x9854 | TX clock Lane 2 |
| 3 | 0xa054 | TX clock Lane 3 |

### TX 驱动控制寄存器

用于配置 DP TX 的电压摆动和预加重：

```c
// 每个 Lane 有一组 TX 驱动控制寄存器
// 基础偏移: 0x8000 + 0x810 = 0x8810
// Lane n 偏移: 0x8810 + n * 0x800

// TX 驱动控制寄存器组
#define TRSV_REG0204   0x0810  // TX drive control 0
#define TRSV_REG0205   0x0814  // TX drive control 1
#define TRSV_REG0206   0x0818  // TX drive control 2
#define TRSV_REG0207   0x081c  // TX drive control 3

// 每个寄存器的值取决于 DP 带宽和电压摆动级别
// 参考 rk3588_dp_tx_drv_ctrl_rbr_hbr, hbr2, hbr3
```

## PCS 寄存器布局 (Base + 0x4000)

### PCS 主要寄存器

```c
// PCS 基础偏移
#define UDPHY_PCS                      0x4000

// PCS 寄存器示例 (具体定义参考 TRM)
#define PCS_RESET_CONTROL              0x4000  // 复位控制
#define PCS_POWER_DOWN_CONTROL         0x4004  // 电源控制
#define PCS_LANE0_CONTROL              0x4010  // Lane 0 控制
#define PCS_LANE1_CONTROL              0x4014  // Lane 1 控制
#define PCS_LANE2_CONTROL              0x4018  // Lane 2 控制
#define PCS_LANE3_CONTROL              0x401c  // Lane 3 控制
```

## GRF 寄存器

### USBDP PHY GRF (@ 0xfd5c8000)

```c
#define USBDPPHY_GRF_BASE              0xfd5c8000

// 低功耗控制
#define USBDPPHY_LOW_PWRN_OFFSET       0x0004
#define USBDPPHY_RX_LFPS               BIT(14)  // RX LFPS enable
#define USBDPPHY_LOW_PWRN              BIT(13)  // Low power mode (1=power up)
```

### USB GRF (@ 0xfd5ac000)

```c
#define USB_GRF_BASE                   0xfd5ac000

// USB3 OTG0 配置
#define USB3OTG0_CFG_OFFSET            0x001c
// Bits [15:0]: 配置值
//   0x0188: 启用 USB3 (bit 15:0 = 1, bit 10:8 = 0)
//   0x1100: 禁用 USB3 (bit 15:0 = 1, bit 12:8 = 1)

// USB3 OTG1 配置
#define USB3OTG1_CFG_OFFSET            0x0034
```

### USB2 PHY GRF (@ 0xfd5d0000)

```c
#define U2PHY_GRF_BASE                 0xfd5d0000

// BVALID 控制
#define U2PHY_BVALID_PHY_CON_OFFSET    0x0008
#define U2PHY_BVALID_GRF_CON_OFFSET    0x0010
```

### VO GRF (@ 0xfd5f0000)

```c
#define VO_GRF_BASE                    0xfd5f0000

// DP AUX 和 lane 选择
#define RK3588_GRF_VO0_CON0            0x0000  // PHY0
#define RK3588_GRF_VO0_CON2            0x0008  // PHY1

// 位字段
#define DP_SINK_HPD_CFG                BIT(11)  // HPD 配置
#define DP_SINK_HPD_SEL                BIT(10)  // HPD 选择
#define DP_AUX_DIN_SEL                 BIT(9)   // AUX data in polarity
#define DP_AUX_DOUT_SEL                BIT(8)   // AUX data out polarity
#define DP_LANE_SEL_N(n)              GENMASK(2*(n)+1, 2*(n))  // Lane n 选择
```

## 寄存器访问示例

### 读取 PLL 锁定状态

```rust
let pma_base = 0xfed80000 + 0x8000;

// 读取 LCPLL 状态
let lcpll_reg = unsafe { ((pma_base + 0x0350) as *const u32).read_volatile() };
let lcpll_locked = (lcpll_reg >> 7) & 0x1 == 1;

// 读取 ROPLL 状态
let ropll_reg = unsafe { ((pma_base + 0x0354) as *const u32).read_volatile() };
let ropll_locked = (ropll_reg >> 1) & 0x1 == 1;
```

### 配置 Lane Multiplexing

```rust
// 配置 Lane 0,1 为 USB，Lane 2,3 为 DP (Combo 模式)
let lane_mux_reg = unsafe { &mut *((pma_base + 0x0288) as *mut u32) };

// 构造寄存器值
let value = (0 << 4) |  // Lane 0: USB
             (0 << 5) |  // Lane 1: USB
             (1 << 6) |  // Lane 2: DP
             (1 << 7) |  // Lane 3: DP
             (1 << 0) |  // Enable Lane 0
             (1 << 1) |  // Enable Lane 1
             (1 << 2) |  // Enable Lane 2
             (1 << 3);   // Enable Lane 3

unsafe { lane_mux_reg.write_volatile(value); }
```

### 配置 DP 复位

```rust
let dp_rstn_reg = unsafe { &mut *((pma_base + 0x038c) as *mut u32) };

// 解除 DP init 复位 (bit 3 = 1)
let value = 1 << 3;
unsafe { dp_rstn_reg.write_volatile(value); }
```

## 内存映射寄存器结构

```c
// PMA 寄存器结构 (部分)
struct usbdp_phy_pma_regs {
    // 0x8000 - 0x827F: CMN 寄存器
    u32 reserved0[0x0A];

    // 0x8288: Lane Mux 和 Enable
    u32 cmn_lane_mux_and_en;

    // 0x828C: DP Link 配置
    u32 cmn_dp_link;

    // ... 其他 CMN 寄存器 ...

    // 0x8350: LCPLL 状态
    u32 cmn_ana_lcpll_done;

    // 0x8354: ROPLL 状态
    u32 cmn_ana_ropll_done;

    // ... 更多寄存器 ...

    // 0x838C: DP 复位控制
    u32 cmn_dp_rstn;

    // TRSV Lane 0 寄存器 (0x8800 - 0x8FFF)
    u32 trsv_ln0_reserved[0x0E];

    // 0x8B84: Lane 0 CDR 状态
    u32 trsv_ln0_mon_rx_cdr_done;

    // TRSV Lane 1 寄存器 (0x9000 - 0x97FF)
    u32 trsv_ln1_reserved[0x200];

    // TRSV Lane 2 寄存器 (0x9800 - 0x9FFF)
    u32 trsv_ln2_reserved[0x200];

    // 0x9B84: Lane 2 CDR 状态
    u32 trsv_ln2_mon_rx_cdr_done;

    // TRSV Lane 3 寄存器 (0xA000 - 0xA7FF)
    u32 trsv_ln3_reserved[0x200];
};
```

## 初始化序列寄存器

### 24MHz 参考时钟配置序列

```c
// 参考: rk3588_udphy_24m_refclk_cfg[]
static const struct reg_sequence rk3588_udphy_24m_refclk_cfg[] = {
    {0x0090, 0x68}, {0x0094, 0x68},
    {0x0128, 0x24}, {0x012c, 0x44},
    // ... 更多寄存器配置 ...
    {0x1a64, 0xa8}
};
```

### 初始化序列

```c
// 参考: rk3588_udphy_init_sequence[]
static const struct reg_sequence rk3588_udphy_init_sequence[] = {
    {0x0104, 0x44}, {0x0234, 0xE8},
    {0x0248, 0x44}, {0x028C, 0x18},
    // ... 更多寄存器配置 ...
    {0x0024, 0x6e},
};
```

## 时序图

```
初始化时序:
│
├─ 退出低功耗 (GRF[0x0004] bit[13] = 1)
│
├─ Deassert APB 复位
│  ├─ pcs_apb (43)
│  └─ pma_apb (1154)
│
├─ 配置 24MHz refclk (寄存器序列)
│
├─ 配置 init sequence (寄存器序列)
│
├─ 配置 lane mux (PMA[0x8288])
│
├─ Deassert PHY 复位
│  ├─ init (40)
│  ├─ cmn (41)
│  └─ lane (42)
│
└─ 等待 PLL 锁定
   ├─ 检查 LCPLL (PMA[0x8350] bit[7])
   └─ 检查 ROPLL (PMA[0x8354] bit[1])
```

## 相关文档

- `phy-rockchip-usbdp.c` (Linux)
- `phy-rockchip-usbdp.c` (U-Boot)
- RK3588 TRM (USBDP PHY 章节)
- USB 3.0 Specification
- DisplayPort 1.4 Specification
