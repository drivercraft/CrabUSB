use core::cell::{Cell, UnsafeCell};

use alloc::vec::Vec;
use usb_if::host::USBError;
use xhci::ExtendedCapability;
use xhci::extended_capabilities::{List, usb_legacy_support_capability::UsbLegacySupport};
use xhci::ring::trb::event::CommandCompletion;

use super::Device;
use super::reg::{MemMapper, XhciRegisters};
use crate::backend::ty::EventHandlerOp;
use crate::backend::xhci::event::EventRing;
use crate::backend::xhci::ring::SendRing;
use crate::osal::SpinWhile;
use crate::queue::Finished;
use crate::{Mmio, backend::ty::HostOp, err::Result};

pub struct Xhci {
    reg: XhciRegisters,
    dma_mask: usize,
    cmd: SendRing<CommandCompletion>,
    inited: Option<Inited>,
    event_handler: Option<EventHandler>,
}

unsafe impl Send for Xhci {}

impl Xhci {
    pub fn new(mmio: Mmio, dma_mask: usize) -> Result<Self> {
        let cmd: SendRing<CommandCompletion> =
            SendRing::new(dma_api::Direction::Bidirectional, dma_mask as _)?;
        let cmd_finished = cmd.finished_handle();
        let event_ring = EventRing::new(dma_mask)?;
        let reg = XhciRegisters::new(mmio);

        Ok(Xhci {
            reg: reg.clone(),
            dma_mask,
            cmd,
            inited: None,
            event_handler: Some(EventHandler::new(reg, cmd_finished, event_ring)),
        })
    }
}

impl HostOp for Xhci {
    type Device = Device;
    type EventHandler = EventHandler;

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

    fn create_event_handler(&mut self) -> Self::EventHandler {
        self.event_handler
            .take()
            .expect("Event handler can only be created once")
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

        SpinWhile::new(|| !self.reg.operational.usbsts.read_volatile().hc_halted()).await;

        debug!("Halted");
        debug!("Wait for ready...");

        SpinWhile::new(|| {
            self.reg
                .operational
                .usbsts
                .read_volatile()
                .controller_not_ready()
        })
        .await;

        debug!("Ready");

        let o = &mut self.reg.operational;
        o.usbcmd.update_volatile(|f| {
            f.set_host_controller_reset();
        });

        debug!("Reset HC");

        SpinWhile::new(|| {
            self.reg
                .operational
                .usbcmd
                .read_volatile()
                .host_controller_reset()
                || self
                    .reg
                    .operational
                    .usbsts
                    .read_volatile()
                    .controller_not_ready()
        })
        .await;

        debug!("Reset finish");

        Ok(())
    }

    fn extended_capabilities(&self) -> Vec<ExtendedCapability<MemMapper>> {
        let hccparams1 = self.reg.capability.hccparams1.read_volatile();
        let mapper = MemMapper {};
        let mut out = Vec::new();
        let mut l = match unsafe { List::new(self.reg.mmio_base, hccparams1, mapper) } {
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

    fn inited(&self) -> Result<&Inited> {
        self.inited.as_ref().ok_or(USBError::NotInitialized)
    }
}

struct Inited {
    reg: XhciRegisters,
    dma_mask: usize,
}

impl Inited {
    fn new(max_slots: usize, reg: XhciRegisters, dma_mask: usize) -> Result<Self> {
        Ok(Self { reg, dma_mask })
    }
}

pub struct EventHandler {
    reg: UnsafeCell<XhciRegisters>,
    cmd_finished: Finished<CommandCompletion>,
    event_ring: UnsafeCell<EventRing>,
}

unsafe impl Send for EventHandler {}
unsafe impl Sync for EventHandler {}

impl EventHandler {
    fn new(
        reg: XhciRegisters,
        cmd_finished: Finished<CommandCompletion>,
        event_ring: EventRing,
    ) -> Self {
        Self {
            reg: UnsafeCell::new(reg),
            cmd_finished,
            event_ring: UnsafeCell::new(event_ring),
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn event_ring(&self) -> &mut EventRing {
        unsafe { &mut *self.event_ring.get() }
    }

    #[allow(clippy::mut_from_ref)]
    fn reg(&self) -> &mut XhciRegisters {
        unsafe { &mut *self.reg.get() }
    }

    fn clean_event_ring(&self) {
        use xhci::ring::trb::event::Allowed;

        while let Some(allowed) = self.event_ring().next() {
            match allowed {
                Allowed::CommandCompletion(c) => {
                    let addr = c.command_trb_pointer();
                    // trace!("[Command] << {allowed:?} @{addr:X}");
                    self.cmd_finished.set_finished(addr.into(), c);
                }
                Allowed::PortStatusChange(_st) => {
                    // debug!("port change: {}", st.port_id());
                }
                Allowed::TransferEvent(c) => {}
                _ => {
                    // debug!("unhandled event {allowed:?}");
                }
            }
        }
    }
}

impl EventHandlerOp for EventHandler {
    fn handle_event(&self) {
        let erdp = {
            self.clean_event_ring();
            self.event_ring().erdp()
        };
        {
            let mut irq = self.reg().interrupter_register_set.interrupter_mut(0);
            irq.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
                r.clear_event_handler_busy();
            });

            irq.iman.update_volatile(|r| {
                r.clear_interrupt_pending();
            });
        }
    }
}
