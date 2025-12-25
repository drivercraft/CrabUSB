use core::cell::UnsafeCell;

use alloc::vec::Vec;
use mbarrier::mb;
use usb_if::host::USBError;
use xhci::ExtendedCapability;
use xhci::extended_capabilities::{List, usb_legacy_support_capability::UsbLegacySupport};
use xhci::registers::doorbell;
use xhci::ring::trb::event::CommandCompletion;

use super::Device;
use super::reg::{MemMapper, XhciRegisters};
use crate::backend::ty::EventHandlerOp;
use crate::backend::xhci::context::{DeviceContextList, ScratchpadBufferArray};
use crate::backend::xhci::event::{EventRing, EventRingInfo};
use crate::backend::xhci::ring::SendRing;
use crate::osal::SpinWhile;
use crate::queue::Finished;
use crate::{Mmio, backend::ty::HostOp, err::Result};

pub struct Xhci {
    reg: XhciRegisters,
    dma_mask: usize,
    cmd: SendRing<CommandCompletion>,
    dev_ctx: Option<DeviceContextList>,
    event_handler: Option<EventHandler>,
    event_ring_info: EventRingInfo,
    scratchpad_buf_arr: Option<ScratchpadBufferArray>,
}

unsafe impl Send for Xhci {}
unsafe impl Sync for Xhci {}

impl Xhci {
    pub fn new(mmio: Mmio, dma_mask: usize) -> Result<Self> {
        let cmd: SendRing<CommandCompletion> =
            SendRing::new(dma_api::Direction::Bidirectional, dma_mask as _)?;
        let cmd_finished = cmd.finished_handle();
        let event_ring = EventRing::new(dma_mask)?;
        let reg = XhciRegisters::new(mmio);
        let event_ring_info = event_ring.info();

        Ok(Xhci {
            reg: reg.clone(),
            dma_mask,
            cmd,
            dev_ctx: None,
            event_handler: Some(EventHandler::new(reg, cmd_finished, event_ring)),
            event_ring_info,
            scratchpad_buf_arr: None,
        })
    }
}

impl HostOp for Xhci {
    type Device = Device;
    type EventHandler = EventHandler;

    async fn init(&mut self) -> Result {
        self.disable_irq();
        // 4.2 Host Controller Initialization
        self.init_ext_caps().await?;
        // After Chip Hardware Reset6 wait until the Controller Not Ready (CNR) flag
        // in the USBSTS is ‘0’ before writing any xHC Operational or Runtime
        // registers.
        self.chip_hardware_reset().await?;

        self.disable_irq();

        // Program the Max Device Slots Enabled (MaxSlotsEn) field in the CONFIG
        // register (5.4.7) to enable the device slots that system software is going to
        // use.
        let max_slots = self.setup_max_device_slots();
        self.dev_ctx = Some(DeviceContextList::new(max_slots as _, self.dma_mask)?);

        // Program the Device Context Base Address Array Pointer (DCBAAP)
        // register (5.4.6) with a 64-bit address pointing to where the Device
        // Context Base Address Array is located.
        self.setup_dcbaap()?;

        // Define the Command Ring Dequeue Pointer by programming the
        // Command Ring Control Register (5.4.5) with a 64-bit address pointing to
        // the starting address of the first TRB of the Command Ring.
        self.set_cmd_ring()?;
        self.init_irq()?;
        self.setup_scratchpads()?;
        // At this point, the host controller is up and running and the Root Hub ports
        // (5.4.8) will begin reporting device connects, etc., and system software may begin
        // enumerating devices. System software may follow the procedures described in
        // section 4.3, to enumerate attached devices.
        self.start();
        mb();

        self.wait_for_running().await;

        self.enable_irq();

        self.reset_ports().await;

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

    fn dev(&self) -> Result<&DeviceContextList> {
        self.dev_ctx.as_ref().ok_or(USBError::NotInitialized)
    }

    fn dev_mut(&mut self) -> Result<&mut DeviceContextList> {
        self.dev_ctx.as_mut().ok_or(USBError::NotInitialized)
    }

    pub fn disable_irq(&mut self) {
        debug!("Disable interrupts");
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.clear_interrupter_enable();
        });
    }

    pub fn enable_irq(&mut self) {
        debug!("Enable interrupts");
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.set_interrupter_enable();
        });
    }

    fn setup_dcbaap(&mut self) -> Result {
        let dcbaa_addr = self.dev()?.dcbaa.bus_addr();
        debug!("DCBAAP: {dcbaa_addr:X}");
        self.reg.operational.dcbaap.update_volatile(|r| {
            r.set(dcbaa_addr);
        });
        Ok(())
    }

    fn set_cmd_ring(&mut self) -> Result {
        let crcr = self.cmd.bus_addr();
        let cycle = self.cmd.cycle();

        debug!("CRCR: {crcr:?}");
        self.reg.operational.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(crcr.into());
            if cycle {
                r.set_ring_cycle_state();
            } else {
                r.clear_ring_cycle_state();
            }
        });

        Ok(())
    }

    fn init_irq(&mut self) -> Result {
        let erstz = self.event_ring_info.erstz;
        let erdp = self.event_ring_info.erdp;
        let erstba = self.event_ring_info.erstba;

        {
            let mut ir0 = self.reg.interrupter_register_set.interrupter_mut(0);

            debug!("ERDP: {erdp:x}");

            ir0.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
                r.set_dequeue_erst_segment_index(0);
                r.clear_event_handler_busy();
            });

            debug!("ERSTZ: {erstz:x}");
            ir0.erstsz.update_volatile(|r| r.set(erstz as _));
            debug!("ERSTBA: {erstba:X}");
            ir0.erstba.update_volatile(|r| {
                r.set(erstba);
            });

            ir0.imod.update_volatile(|im| {
                im.set_interrupt_moderation_interval(0x1F);
                im.set_interrupt_moderation_counter(0);
            });
        }

        {
            debug!("Enabling primary interrupter.");
            self.reg
                .interrupter_register_set
                .interrupter_mut(0)
                .iman
                .update_volatile(|im| {
                    im.set_interrupt_enable();
                    im.clear_interrupt_pending();
                });
        }

        /* Set the HCD state before we enable the irqs */
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.set_host_system_error_enable();
            r.set_enable_wrap_event();
        });
        Ok(())
    }

    fn setup_scratchpads(&mut self) -> Result {
        let scratchpad_buf_arr = {
            let buf_count = {
                let count = self
                    .reg
                    .capability
                    .hcsparams2
                    .read_volatile()
                    .max_scratchpad_buffers();
                debug!("Scratch buf count: {count}");
                count
            };
            if buf_count == 0 {
                return Ok(());
            }
            let scratchpad_buf_arr = ScratchpadBufferArray::new(buf_count as _, self.dma_mask)?;

            let bus_addr = scratchpad_buf_arr.bus_addr();

            self.dev_mut()?.dcbaa.set(0, bus_addr);

            debug!("Setting up {buf_count} scratchpads, at {bus_addr:#0x}");
            scratchpad_buf_arr
        };

        self.scratchpad_buf_arr = Some(scratchpad_buf_arr);

        Ok(())
    }

    fn start(&mut self) {
        self.reg.operational.usbcmd.update_volatile(|r| {
            r.set_run_stop();
        });
        debug!("Start run");
    }

    async fn wait_for_running(&mut self) {
        SpinWhile::new(|| {
            let sts = self.reg.operational.usbsts.read_volatile();
            sts.hc_halted() || sts.controller_not_ready()
        })
        .await;

        info!("Running");

        self.reg
            .doorbell
            .write_volatile_at(0, doorbell::Register::default());
    }

    async fn reset_ports(&mut self) {
        let regs = &mut self.reg;
        let port_len = regs.port_register_set.len();

        for i in 0..port_len {
            debug!("Port {i} start reset",);
            regs.port_register_set.update_volatile_at(i, |port| {
                port.portsc.set_0_port_enabled_disabled();
                port.portsc.set_port_reset();
            });
        }
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
                    debug!("port change: {}", _st.port_id());
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
