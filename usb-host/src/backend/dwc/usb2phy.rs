//! RK3588 USB2 PHY 最小化驱动
//!
//! 这个模块提供 USB2 PHY 的最小化初始化，只用于启动时钟输出。
//! USB2 PHY 输出 480MHz 时钟给 DWC3 控制器，这是 PHY 寄存器可访问的必要条件。

use crate::Mmio;

/// RK3588 USB2 PHY 寄存器偏移
#[allow(dead_code)]
#[repr(u32)]
enum RegOffset {
    /// 主机/设备配置寄存器
    Usb2PhyCfg = 0x00,
}

/// USB2 PHY 最小化驱动
pub struct Usb2Phy {
    base: usize,
}

impl Usb2Phy {
    /// 创建新的 USB2 PHY 实例
    ///
    /// # Safety
    ///
    /// 调用者必须确保 `mmio_base` 指向有效的内存映射寄存器区域
    pub unsafe fn new(mmio_base: Mmio) -> Self {
        Self {
            base: mmio_base.as_ptr() as usize,
        }
    }

    /// 最小化初始化（仅用于启动时钟）
    ///
    /// USB2 PHY 需要输出 480MHz 时钟给 DWC3 控制器。
    ///
    /// 注意：这是一个最小化实现，只做最基本的事情来启动时钟。
    /// 完整的 USB2 功能需要完整的 PHY 初始化。
    pub fn init_minimal(&self) {
        log::info!("USB2PHY@{:x}: Minimal initialization (clock enable only)", self.base);

        // 读取当前配置
        let cfg_reg = unsafe { (self.base + RegOffset::Usb2PhyCfg as usize) as *const u32 };
        let cfg_val = unsafe { cfg_reg.read_volatile() };

        log::debug!("USB2PHY@{:x}: CFG before: {:#08x}", self.base, cfg_val);

        // RK3588 USB2 PHY 在复位后应该自动启动时钟输出
        // 我们不需要做任何特殊配置，只需确保 PHY 不在低功耗模式

        // 检查 PHY 是否处于低功耗模式
        // bit[15] = PHY_SUSPEND
        let phy_suspend = (cfg_val >> 15) & 0x1;

        if phy_suspend != 0 {
            log::warn!("USB2PHY@{:x}: PHY is in suspend mode, attempting to wake", self.base);

            // 写入 0 来退出 suspend 模式
            // 注意：具体实现可能需要更复杂的操作
            unsafe {
                let reg = (self.base + RegOffset::Usb2PhyCfg as usize) as *mut u32;
                reg.write_volatile(cfg_val & !(1 << 15));
            }

            log::info!("USB2PHY@{:x}: Woke from suspend mode", self.base);
        } else {
            log::info!("USB2PHY@{:x}: PHY is active (not suspended)", self.base);
        }

        // 读取配置后的值
        let cfg_after = unsafe { cfg_reg.read_volatile() };
        log::debug!("USB2PHY@{:x}: CFG after: {:#08x}", self.base, cfg_after);

        log::info!("✓ USB2PHY@{:x}: Minimal init complete (480MHz clock should be running)", self.base);
    }
}
