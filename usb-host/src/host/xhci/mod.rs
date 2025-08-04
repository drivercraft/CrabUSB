use core::{num::NonZeroUsize, ptr::NonNull, time::Duration};

use alloc::vec::Vec;
use futures::task::AtomicWaker;
use log::*;
use xhci::{
    ExtendedCapability,
    accessor::Mapper,
    extended_capabilities::{self, usb_legacy_support_capability::UsbLegacySupport},
    ring::trb::{command, event::CommandCompletion},
};

mod context;
mod def;
mod device;
mod endpoint;
mod event;
mod interface;
mod reg;
mod ring;
mod root;

use super::Controller;
use crate::{
    err::*,
    sleep,
    xhci::{reg::XhciRegisters, root::RootHub},
};
use def::*;
pub use device::Device;

pub struct Xhci {
    reg: XhciRegisters,
    root: Option<RootHub>,
    port_wake: AtomicWaker,
}

unsafe impl Send for Xhci {}

impl Controller for Xhci {
    async fn init(&mut self) -> Result {
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
        let root_hub = RootHub::new(max_slots as _, self.reg.clone())?;
        root_hub.init()?;
        self.root = Some(root_hub);
        trace!("Root hub initialized with max slots: {max_slots}");
        self.root()?.lock().enable_irq();
        self.root()?.wait_for_running().await;
        self.root()?.lock().reset_ports();
        Ok(())
    }

    async fn test_cmd(&mut self) -> Result {
        self.post_cmd(command::Allowed::Noop(command::Noop::new()))
            .await?;
        Ok(())
    }

    fn handle_irq(&mut self) {
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
                debug!("Port Change Detected");
                if let Some(data) = self.port_wake.take() {
                    data.wake();
                }

                sts.clear_port_change_detect();
            }

            if sts.host_system_error() {
                debug!("Host System Error");
                sts.clear_host_system_error();
            }

            self.reg.operational.usbsts.write_volatile(sts);
        }
    }

    async fn probe(&mut self) -> Result<Vec<Self::Device>> {
        let mut slots = Vec::new();
        let port_idx_list = self.port_idx_list();

        for idx in port_idx_list {
            let slot = self.root()?.new_device(idx).await?;
            slots.push(slot);
        }
        Ok(slots)
    }

    type Device = device::Device;
}

impl Xhci {
    pub fn new(mmio_base: NonNull<u8>) -> Self {
        Self {
            reg: XhciRegisters::new(mmio_base),
            root: None,
            port_wake: AtomicWaker::new(),
        }
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

        // debug!("Is 64 bit {}", self.is_64_byte());
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

    async fn post_cmd(&mut self, trb: command::Allowed) -> Result<CommandCompletion> {
        self.root()?.post_cmd(trb).await
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

            if !portsc.port_enabled_disabled() {
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
        1 | 3 => 64,
        2 => 8,
        4 => 512,
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
