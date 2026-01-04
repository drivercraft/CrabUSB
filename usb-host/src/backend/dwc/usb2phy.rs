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
    ///
    /// ## UTMI 时钟验证
    ///
    /// 参考 u-boot 初始化流程，USB2 PHY 初始化后应该输出 UTMI 480MHz 时钟。
    /// 这里检查 PHY 的基本状态以验证时钟可能正在运行。
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

        // ⚠️ 新增：验证 UTMI 时钟状态
        // USB2 PHY 初始化后应该输出 480MHz UTMI 时钟
        // 检查 PHY 的关键位以验证时钟可能正在运行：
        // - bit[15]: PHY_SUSPEND (应该为 0)
        // - bit[1]: PORT_ENABLE (可能被 GRF 控制)
        // - bit[0]: PORT_SUSPEND (可能被 GRF 控制)
        let suspend_after = (cfg_after >> 15) & 0x1;
        let port_enable = (cfg_after >> 1) & 0x1;
        let port_suspend = cfg_after & 0x1;

        log::info!("USB2PHY@{:x}: PHY status check:", self.base);
        log::info!("  - PHY_SUSPEND (bit[15]): {} (0=active, 1=suspend)", suspend_after);
        log::info!("  - PORT_ENABLE (bit[1]):  {}", port_enable);
        log::info!("  - PORT_SUSPEND (bit[0]): {}", port_suspend);

        // 如果 PHY 不在挂起模式，UTMI 时钟应该正在运行
        if suspend_after == 0 {
            log::info!("✓ USB2PHY@{:x}: PHY is active - UTMI 480MHz clock should be running", self.base);
            log::info!("✓ USB2PHY@{:x}: Minimal init complete (480MHz clock should be running)", self.base);
        } else {
            log::warn!("⚠ USB2PHY@{:x}: PHY is still in suspend mode - UTMI clock may not be running!", self.base);
        }
    }

    /// 验证 UTMI 时钟状态
    ///
    /// 检查 USB2 PHY 是否正在运行，这间接表明 UTMI 480MHz 时钟可能正在输出。
    ///
    /// 注意：这是一个简化的检查。完整的验证需要检查 USB2PHY GRF 的端口使能状态。
    pub fn verify_utmi_clock(&self) -> bool {
        let cfg_reg = unsafe { (self.base + RegOffset::Usb2PhyCfg as usize) as *const u32 };
        let cfg_val = unsafe { cfg_reg.read_volatile() };

        // PHY 不在挂起模式表示时钟可能在运行
        let phy_suspend = (cfg_val >> 15) & 0x1;

        if phy_suspend == 0 {
            log::debug!("USB2PHY@{:x}: UTMI clock verification passed - PHY is active", self.base);
            true
        } else {
            log::warn!("USB2PHY@{:x}: UTMI clock verification failed - PHY is suspended", self.base);
            false
        }
    }
}
