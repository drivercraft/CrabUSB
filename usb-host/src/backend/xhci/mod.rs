use core::{num::NonZeroUsize, ptr::NonNull, time::Duration};

use alloc::{boxed::Box, vec::Vec};
use futures::{FutureExt, task::AtomicWaker};
use xhci::{
    ExtendedCapability,
    accessor::Mapper,
    extended_capabilities::{self, usb_legacy_support_capability::UsbLegacySupport},
};

mod context;
mod def;
mod delay;
mod device;
pub mod dwc3;
mod endpoint;
mod event;
mod interface;
mod reg;
mod ring;
pub mod rk3588_phy;
mod root;

use crate::{
    backend::xhci::{reg::XhciRegisters, root::RootHub},
    err::*,
    osal::kernel::sleep,
};
use def::*;

pub struct Xhci {
    mmio_base: NonNull<u8>,
    reg: XhciRegisters,
    root: Option<RootHub>,
    port_wake: AtomicWaker,
    dma_mask: usize,
}

unsafe impl Send for Xhci {}

impl usb_if::host::Controller for Xhci {
    fn init(
        &'_ mut self,
    ) -> futures::future::LocalBoxFuture<'_, core::result::Result<(), usb_if::host::USBError>> {
        async {
            self.init_dwc3_if_needed();
            self.init_ext_caps().await?;
            self.chip_hardware_reset().await?;

            self.reg = XhciRegisters::new(self.mmio_base);

            let max_slots = self.setup_max_device_slots();
            let root_hub = RootHub::new(max_slots as _, self.reg.clone(), self.dma_mask)?;
            root_hub.init()?;
            self.root = Some(root_hub);
            self.root()?.wait_for_running().await;
            
            // GL3523 hub workaround: Toggle VBUS after xHCI is running
            // This resets the hub so it sees immediate host activity when it powers up.
            // Without this, the hub enters standby mode and doesn't respond to Rx.Detect.
            self.toggle_vbus_if_rk3588().await;
            
            self.root()?.lock().enable_irq();
            self.root()?.lock().reset_ports();
            sleep(Duration::from_millis(100)).await;
            Ok(())
        }
        .boxed_local()
    }

    fn device_list(
        &'_ self,
    ) -> futures::future::LocalBoxFuture<
        '_,
        core::result::Result<Vec<Box<dyn usb_if::host::DeviceInfo>>, usb_if::host::USBError>,
    > {
        async {
            let mut slots = Vec::new();
            let port_idx_list = self.port_idx_list();

            for idx in port_idx_list {
                let slot = self.root()?.new_device(idx).await?;
                slots.push(Box::new(slot) as Box<dyn usb_if::host::DeviceInfo>);
            }
            Ok(slots)
        }
        .boxed_local()
    }

    fn handle_event(&mut self) {
        unsafe {
            let mut sts = self.reg.operational.usbsts.read_volatile();
            if sts.event_interrupt() {
                if let Some(root) = self.root.as_mut() {
                    root.force_use().handle_event();
                } else {
                    warn!("[XHCI] Not initialized, cannot handle event");
                }

                sts.clear_event_interrupt();
            }
            if sts.port_change_detect() {
                // debug!("Port Change Detected");
                if let Some(data) = self.port_wake.take() {
                    data.wake();
                }

                sts.clear_port_change_detect();
            }

            if sts.host_system_error() {
                // debug!("Host System Error");
                sts.clear_host_system_error();
            }

            self.reg.operational.usbsts.write_volatile(sts);
        }
    }
}

impl Xhci {
    pub fn new(mmio_base: NonNull<u8>, dma_mask: usize) -> Box<Self> {
        Box::new(Self {
            mmio_base,
            reg: XhciRegisters::new(mmio_base),
            root: None,
            port_wake: AtomicWaker::new(),
            dma_mask,
        })
    }

    fn init_dwc3_if_needed(&self) {
        if unsafe { dwc3::is_dwc3_xhci(self.mmio_base) } {
            debug!("Detected DWC3-based XHCI controller");
        }
    }
    
    /// Toggle VBUS power if this is an RK3588 USB3_1 controller
    /// 
    /// This is a workaround for the GL3523 USB hub cold-start issue on Orange Pi 5 Plus.
    /// The hub enters a non-responsive standby state if VBUS is present but no USB host
    /// activity occurs within ~1-2 seconds. By toggling VBUS after the xHCI controller
    /// is running, we reset the hub so it sees immediate host activity when it powers up.
    #[cfg(feature = "aggressive_usb_reset")]
    async fn toggle_vbus_if_rk3588(&self) {
        let base_addr = self.mmio_base.as_ptr() as usize;
        
        if rk3588_phy::is_rk3588_usb3_port1(base_addr) {
            debug!("RK3588 USB3_1: Applying GL3523 hub VBUS toggle workaround");
            
            unsafe {
                rk3588_phy::toggle_vbus_port1(
                    rk3588_phy::VBUS_OFF_MS,
                    rk3588_phy::VBUS_ON_WAIT_MS,
                );
            }
            
            sleep(Duration::from_millis(200)).await;
        }
    }
    
    /// No-op variant when aggressive_usb_reset feature is disabled
    #[cfg(not(feature = "aggressive_usb_reset"))]
    async fn toggle_vbus_if_rk3588(&self) {
        // VBUS toggle disabled - feature flag not set
    }

    async fn chip_hardware_reset(&mut self) -> Result {
        debug!("Reset begin ...");
        
        self.reg.operational.usbcmd.update_volatile(|c| {
            c.clear_run_stop();
        });

        while !self.reg.operational.usbsts.read_volatile().hc_halted() {
            sleep(Duration::from_millis(10)).await;
        }

        debug!("Halted");
        let o = &mut self.reg.operational;
        debug!("Wait for ready...");
        while o.usbsts.read_volatile().controller_not_ready() {
            sleep(Duration::from_millis(10)).await;
        }
        debug!("Ready");

        o.usbcmd.update_volatile(|f| {
            f.set_host_controller_reset();
        });

        debug!("Reset HC");
        while o.usbcmd.read_volatile().host_controller_reset()
            || o.usbsts.read_volatile().controller_not_ready()
        {
            sleep(Duration::from_millis(10)).await;
        }
        debug!("Reset finish");

        Ok(())
    }

    fn setup_max_device_slots(&mut self) -> u8 {
        let regs = &mut self.reg;
        let max_slots = regs
            .capability
            .hcsparams1
            .read_volatile()
            .number_of_device_slots();

        regs.operational.config.update_volatile(|r| {
            r.set_max_device_slots_enabled(max_slots);
        });

        debug!("Max device slots: {max_slots}");

        max_slots
    }

    fn extended_capabilities(&self) -> Vec<ExtendedCapability<MemMapper>> {
        let hccparams1 = self.reg.capability.hccparams1.read_volatile();
        let mapper = MemMapper {};
        let mut out = Vec::new();
        let mut l = match unsafe {
            extended_capabilities::List::new(self.reg.mmio_base, hccparams1, mapper)
        } {
            Some(v) => v,
            None => return out,
        };

        for one in &mut l {
            if let Ok(cap) = one {
                out.push(cap);
            } else {
                break;
            }
        }
        out
    }

    async fn init_ext_caps(&mut self) -> Result {
        let caps = self.extended_capabilities();
        debug!("Extended capabilities: {:?}", caps.len());

        for cap in self.extended_capabilities() {
            if let ExtendedCapability::UsbLegacySupport(usb_legacy_support) = cap {
                self.legacy_init(usb_legacy_support).await?;
            }
        }

        Ok(())
    }

    async fn legacy_init(&mut self, mut usb_legacy_support: UsbLegacySupport<MemMapper>) -> Result {
        debug!("legacy init");
        usb_legacy_support.usblegsup.update_volatile(|r| {
            r.set_hc_os_owned_semaphore();
        });

        loop {
            sleep(Duration::from_millis(100)).await;
            let up = usb_legacy_support.usblegsup.read_volatile();
            if up.hc_os_owned_semaphore() && !up.hc_bios_owned_semaphore() {
                break;
            }
        }

        debug!("claimed ownership from BIOS");

        usb_legacy_support.usblegctlsts.update_volatile(|r| {
            r.clear_usb_smi_enable();
            r.clear_smi_on_host_system_error_enable();
            r.clear_smi_on_os_ownership_enable();
            r.clear_smi_on_pci_command_enable();
            r.clear_smi_on_bar_enable();

            r.clear_smi_on_bar();
            r.clear_smi_on_pci_command();
            r.clear_smi_on_os_ownership_change();
        });

        Ok(())
    }

    fn port_idx_list(&self) -> Vec<usize> {
        let mut port_idx_list = Vec::new();
        let port_len = self.reg.port_register_set.len();
        for i in 0..port_len {
            let portsc = &self.reg.port_register_set.read_volatile_at(i).portsc;
            info!(
                "Port {}: Enabled: {}, Connected: {}, Speed {}, Power {}",
                i,
                portsc.port_enabled_disabled(),
                portsc.current_connect_status(),
                portsc.port_speed(),
                portsc.port_power()
            );

            if !portsc.port_enabled_disabled() || !portsc.current_connect_status() {
                continue;
            }

            port_idx_list.push(i);
        }

        port_idx_list
    }

    fn root(&self) -> Result<&RootHub> {
        self.root.as_ref().ok_or(USBError::NotInitialized)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemMapper;
impl Mapper for MemMapper {
    unsafe fn map(&mut self, phys_start: usize, _bytes: usize) -> NonZeroUsize {
        unsafe { NonZeroUsize::new_unchecked(phys_start) }
    }
    fn unmap(&mut self, _virt_start: usize, _bytes: usize) {}
}

fn parse_default_max_packet_size_from_port_speed(speed: u8) -> u16 {
    match speed {
        1 => 8,
        2 | 3 => 64,
        4..=6 => 512,
        v => unimplemented!("PSI: {}", v),
    }
}
fn append_port_to_route_string(route_string: u32, port_id: usize) -> u32 {
    let mut route_string = route_string;
    for tier in 0..5 {
        if route_string & (0x0f << (tier * 4)) == 0 && tier < 5 {
            route_string |= (port_id as u32) << (tier * 4);
            return route_string;
        }
    }

    route_string
}
