//! DWC3 (DesignWare USB3 Controller) 驱动
//!
//! DWC3 是一个 USB3 DRD (Dual Role Device) 控制器，支持 Host 和 Device 模式。
//! 本模块实现 Host 模式驱动，基于 xHCI 规范。

use alloc::vec::Vec;

use crate::{Mmio, Xhci, backend::ty::HostOp, err::Result};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

mod reg;

use reg::Dwc3Regs;
use tock_registers::interfaces::{ReadWriteable, Readable, Writeable};

/// DWC3 控制器
///
/// DWC3 实际上是 xHCI 主机控制器的封装。在 Host 模式下，
/// DWC3 的 xHCI 寄存器区域 (0x0000 - 0x7fff) 包含标准 xHCI 寄存器，
/// 全局寄存器区域 (0xc100 - 0xcfff) 包含 DWC3 特定配置。
pub struct Dwc {
    xhci: Xhci,
    dwc_regs: Dwc3Regs,
}

impl Dwc {
    /// 创建新的 DWC3 控制器实例
    ///
    /// # 参数
    ///
    /// * `mmio` - MMIO 基址
    /// * `dma_mask` - DMA 掩码
    ///
    /// # 初始化流程
    ///
    /// 1. 验证 SNPSID 寄存器
    /// 2. 设置为 HOST 模式
    /// 3. 初始化 xHCI 主机控制器
    pub fn new(mmio: Mmio, dma_mask: usize) -> Result<Self> {
        let mmio_base = mmio.as_ptr() as usize;
        let xhci = Xhci::new(mmio, dma_mask)?;

        let dwc_regs = unsafe { Dwc3Regs::new(mmio_base) };

        // 验证 SNPSID
        let globals = dwc_regs.globals();
        let snpsid_full = globals.gsnpsid.get();
        let snpsid = globals.gsnpsid.read(reg::GSNPSID::PRODUCT_ID);
        let revision = globals.gsnpsid.read(reg::GSNPSID::REVISION);
        let ip_id = (snpsid_full >> 16) & 0xffff; // 提取高 16 位作为 IP ID

        log::info!(
            "DWC3 SNPSID: full={:#010x}, ip_id={:#06x}, product_id={:#06x}, revision={:#06x}",
            snpsid_full, ip_id, snpsid, revision
        );

        // DWC3 IP ID 验证
        // Linux 中的定义: DWC3_IP=0x5533, DWC31_IP=0x3331, DWC32_IP=0x3332
        // 但某些实现可能有不同的值，如 0x300a (ip_id=0x0003)
        match ip_id {
            0x5533 => {
                log::info!("Detected DWC_usb3 controller");
            }
            0x3331 => {
                log::info!("Detected DWC_usb31 controller");
            }
            0x3332 => {
                log::info!("Detected DWC_usb32 controller");
            }
            _ => {
                log::warn!(
                    "Unknown DWC3 IP ID: {:#06x} (expected 0x5533, 0x3331, or 0x3332)",
                    ip_id
                );
                log::warn!("Continuing with initialization - may be a custom or FPGA variant");
            }
        }

        Ok(Self { xhci, dwc_regs })
    }

    /// 设置 DWC3 为 HOST 模式
    ///
    /// 根据 Linux DWC3 驱动的实现，HOST 模式下：
    /// 1. 设置 GCTL.PRTCAPDIR = DWC3_GCTL_PRTCAP_HOST (1)
    /// 2. 软复位在 HOST 模式下由 xHCI 驱动处理
    fn setup_host_mode(&self) {
        let globals = self.dwc_regs.globals();

        // 读取当前 GCTL 值
        let current = globals.gctl.get();
        log::debug!("Current GCTL: {:#010x}", current);

        // 检查当前 PRTCAPDIR 值 (bits 12-13)
        let current_prtcap = (current >> 12) & 0x3;
        log::debug!("Current PRTCAPDIR: {} (0=Device, 1=Host, 2=Device, 3=OTG)", current_prtcap);

        // 设置为 HOST 模式
        // 使用 modify 方法修改 PRTCAPDIR 字段
        ReadWriteable::modify(
            &globals.gctl,
            reg::GCTL::PRTCAPDIR.val(1) // 1 = Host mode
        );

        // 读取并验证修改后的值
        let updated = globals.gctl.get();
        let updated_prtcap = (updated >> 12) & 0x3;
        log::debug!("Updated GCTL: {:#010x}", updated);
        log::debug!("Updated PRTCAPDIR: {}", updated_prtcap);

        log::info!("DWC3 configured in HOST mode (PRTCAPDIR={})", updated_prtcap);

        // 验证模式切换完成
        let current_mode = globals.gsts.read(reg::GSTS::CURMOD);
        log::info!("DWC3 current GSTS.CURMOD: {} (0=Device, 1=Host)", current_mode);

        if current_mode == 1 {
            log::info!("✓ DWC3 successfully switched to HOST mode");
        } else {
            log::warn!("⚠ DWC3 mode mismatch: expected 1 (Host), got {}", current_mode);
        }
    }

    /// 核心软复位
    ///
    /// 根据 DWC3 规范，在 HOST 模式下软复位由 xHCI 驱动处理。
    /// 此函数为最简实现，仅用于 Device 模式的软复位。
    fn core_soft_reset(&self) {
        // HOST 模式下，软复位由 xHCI 处理
        // 参考: Linux drivers/usb/dwc3/core.c dwc3_core_soft_reset
        log::debug!("DWC3 soft reset skipped (HOST mode, handled by xHCI)");
    }

    /// 配置 PHY
    ///
    /// 配置 USB2 和 USB3 PHY 的基本参数
    fn setup_phy(&self) {
        let globals = self.dwc_regs.globals();

        // 配置 USB2 PHY
        // 启用 SUSPEND PHY
        ReadWriteable::modify(
            &globals.gusb2phycfg0,
            reg::GUSB2PHYCFG::SUSPHY::Enable
        );

        log::debug!("DWC3 USB2 PHY configured");

        // 配置 USB3 PHY
        // 设置为正常模式 (非复位)
        ReadWriteable::modify(
            &globals.gusb3pipectl0,
            reg::GUSB3PIPECTL::PHYSOFTRST::Normal
        );

        log::debug!("DWC3 USB3 PHY configured");
    }
}

impl HostOp for Dwc {
    type DeviceInfo = DeviceInfo;
    type EventHandler = EventHandler;

    /// 初始化 DWC3 控制器
    async fn init(&mut self) -> Result {
        log::info!("Initializing DWC3 controller");

        // 设置 HOST 模式
        self.setup_host_mode();

        // 配置 PHY
        self.setup_phy();

        // 核心软复位 (HOST 模式下由 xHCI 处理)
        self.core_soft_reset();

        // 初始化 xHCI 主机控制器
        self.xhci.init().await?;

        log::info!("DWC3 controller initialized successfully");

        Ok(())
    }

    /// 探测 USB 设备
    async fn probe_devices(&mut self) -> Result<Vec<Self::DeviceInfo>> {
        let devices = self.xhci.probe_devices().await?;
        Ok(devices)
    }

    /// 打开 USB 设备
    async fn open_device(
        &mut self,
        dev: &Self::DeviceInfo,
    ) -> Result<<Self::DeviceInfo as super::ty::DeviceInfoOp>::Device> {
        let device = self.xhci.open_device(dev).await?;
        Ok(device)
    }

    /// 创建事件处理器
    fn create_event_handler(&mut self) -> Self::EventHandler {
        self.xhci.create_event_handler()
    }
}
