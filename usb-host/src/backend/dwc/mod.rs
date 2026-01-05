//! DWC3 (DesignWare USB3 Controller) 驱动
//!
//! DWC3 是一个 USB3 DRD (Dual Role Device) 控制器，支持 Host 和 Device 模式。
//! 本模块实现 Host 模式驱动，基于 xHCI 规范。

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::{
    Mmio, Xhci,
    backend::{
        dwc::{event::EventBuffer, udphy::Udphy},
        ty::HostOp,
    },
    err::Result,
};

pub use crate::backend::xhci::*;

use device::DeviceInfo;
use host::EventHandler;

pub mod grf;
// pub mod phy;
mod consts;
mod event;
mod reg;
mod udphy;
pub mod usb2phy;

// pub use phy::{UsbDpMode, UsbDpPhy, UsbDpPhyConfig};
use consts::*;
use reg::Dwc3Regs;
pub use udphy::UdphyParam;
pub use usb2phy::Usb2Phy;

/// CRU (Clock and Reset Unit)
pub trait CruOp: Sync + Send + 'static {
    fn reset_assert(&self, id: u64);
    fn reset_deassert(&self, id: u64);
}

/// DWC3 控制器
///
/// DWC3 实际上是 xHCI 主机控制器的封装。在 Host 模式下，
/// DWC3 的 xHCI 寄存器区域 (0x0000 - 0x7fff) 包含标准 xHCI 寄存器，
/// 全局寄存器区域 (0xc100 - 0xcfff) 包含 DWC3 特定配置。
pub struct Dwc {
    xhci: Xhci,
    phy: Udphy,
    // usb2_phy: Usb2Phy,
    dwc_regs: Dwc3Regs,
    cru: Arc<dyn CruOp>,
    rsts: BTreeMap<String, u64>,
    ev_buffs: Vec<EventBuffer>,
}

impl Dwc {
    pub fn new(
        ctrl: Mmio,
        phy: Mmio,
        param: UdphyParam<'_>,
        cru: impl CruOp,
        rst_list: &'_ [(&'_ str, u64)],
        dma_mask: usize,
    ) -> Result<Self> {
        let mmio_base = ctrl.as_ptr() as usize;
        let cru = Arc::new(cru);

        let phy = Udphy::new(phy, cru.clone(), param);

        let xhci = Xhci::new(ctrl, dma_mask)?;

        let dwc_regs = unsafe { Dwc3Regs::new(mmio_base) };

        let mut rsts = BTreeMap::new();
        for &(name, id) in rst_list.iter() {
            rsts.insert(String::from(name), id);
        }

        Ok(Self {
            xhci,
            dwc_regs,
            phy,
            cru,
            rsts,
            ev_buffs: vec![],
            // usb2_phy,
        })
    }

    async fn dwc3_init(&mut self) -> Result<()> {
        self.alloc_event_buffers(DWC3_EVENT_BUFFERS_SIZE)?;

        Ok(())
    }

    fn alloc_event_buffers(&mut self, len: usize) -> Result<()> {
        let num_buffs = self.dwc_regs.num_event_buffers();
        debug!("Allocating {} event buffers", num_buffs);
        for _ in 0..num_buffs {
            let ev_buff = EventBuffer::new(len, self.xhci.dma_mask)?;
            self.ev_buffs.push(ev_buff);
        }
        Ok(())
    }
}

impl HostOp for Dwc {
    type DeviceInfo = DeviceInfo;
    type EventHandler = EventHandler;

    /// 初始化 DWC3 控制器
    ///
    /// ## 初始化顺序说明
    ///
    /// 在 HOST 模式下，必须按照以下顺序初始化：
    /// 1. USBDP PHY 硬件初始化（时钟、复位、PLL）
    /// 2. DWC3 全局配置（GCTL、HOST 模式）
    /// 3. **xHCI 主机控制器初始化**（执行 HCRST 复位）
    /// 4. DWC3 PHY 配置寄存器（GUSB3PIPECTL、GUSB2PHYCFG）
    ///
    /// **关键点**：DWC3 PHY 配置寄存器必须在 xHCI 执行 HCRST **之后**才能访问，
    /// 因为 HCRST 会复位并使能 host block 的 PHY 接口。
    async fn init(&mut self) -> Result {
        info!("DWC3: Starting controller initialization");

        /*
         * It must hold whole USB3.0 OTG controller in resetting to hold pipe
         * power state in P2 before initializing TypeC PHY on RK3399 platform.
         */
        for &id in self.rsts.values() {
            self.cru.reset_assert(id);
        }

        crate::osal::kernel::delay(core::time::Duration::from_millis(1));

        // 步骤 2: 配置 USBDP PHY 硬件（时钟、复位、PLL 等）
        info!("DWC3: Step 2 - Configuring USBDP PHY hardware");

        self.phy.init().await?;

        for &id in self.rsts.values() {
            self.cru.reset_deassert(id);
        }

        self.dwc3_init().await?;

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
