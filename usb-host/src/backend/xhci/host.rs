use alloc::vec::Vec;
use xhci::ExtendedCapability;
use xhci::extended_capabilities::{List, usb_legacy_support_capability::UsbLegacySupport};

use super::Device;
use super::reg::{MemMapper, XhciRegisters};
use crate::{Mmio, backend::ty::HostOp, err::Result};

pub struct Xhci {
    reg: XhciRegisters,
    dma_mask: usize,
}

impl Xhci {
    pub fn new(mmio: Mmio, dma_mask: usize) -> Self {
        Xhci {
            reg: XhciRegisters::new(mmio),
            dma_mask,
        }
    }
}

impl HostOp for Xhci {
    type Device = Device;

    async fn initialize(&mut self) -> Result {
        // 4.2 Host Controller Initialization
        self.init_ext_caps().await?;
        // After Chip Hardware Reset6 wait until the Controller Not Ready (CNR) flag
        // in the USBSTS is ‘0’ before writing any xHC Operational or Runtime
        // registers.
        self.chip_hardware_reset().await?;
        // Program the Max Device Slots Enabled (MaxSlotsEn) field in the CONFIG
        // register (5.4.7) to enable the device slots that system software is going to
        // use.
        let max_slots = self.setup_max_device_slots();
        // let root_hub = RootHub::new(max_slots as _, self.reg.clone(), self.dma_mask)?;
        // root_hub.init()?;
        // self.root = Some(root_hub);
        // // trace!("Root hub initialized with max slots: {max_slots}");
        // self.root()?.wait_for_running().await;
        // self.root()?.lock().enable_irq();
        // self.root()?.lock().reset_ports();

        // // Additional delay after port reset for device detection stability
        // // Linux kernel typically waits for device connection stabilization
        // sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    async fn device_list(&self) -> Result<Vec<usb_if::descriptor::DeviceDescriptor>> {
        todo!()
    }

    async fn open_device(
        &mut self,
        desc: &usb_if::descriptor::DeviceDescriptor,
    ) -> Result<Self::Device> {
        todo!()
    }

    fn poll_events(&mut self) {
        todo!()
    }
}

impl Xhci {
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

    async fn chip_hardware_reset(&mut self) -> Result {
        debug!("Reset begin ...");
        self.reg.operational.usbcmd.update_volatile(|c| {
            c.clear_run_stop();
        });

        self.reg
            .wait_for(|r| r.operational.usbsts.read_volatile().hc_halted())
            .await;

        debug!("Halted");
        debug!("Wait for ready...");
        self.reg
            .wait_for(|o| !o.operational.usbsts.read_volatile().controller_not_ready())
            .await;

        debug!("Ready");

        let o = &mut self.reg.operational;
        o.usbcmd.update_volatile(|f| {
            f.set_host_controller_reset();
        });

        debug!("Reset HC");

        self.reg
            .wait_for(|o| {
                !(o.operational.usbcmd.read_volatile().host_controller_reset()
                    || o.operational.usbsts.read_volatile().controller_not_ready())
            })
            .await;

        debug!("Reset finish");

        Ok(())
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

    async fn legacy_init(&mut self, mut usb_legacy_support: UsbLegacySupport<MemMapper>) -> Result {
        debug!("legacy init");
        usb_legacy_support.usblegsup.update_volatile(|r| {
            r.set_hc_os_owned_semaphore();
        });

        loop {
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
}
