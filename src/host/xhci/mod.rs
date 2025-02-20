use core::{hint::spin_loop, num::NonZeroUsize, ptr::NonNull, time::Duration};

use alloc::vec::Vec;
use context::ScratchpadBufferArray;
use future::LocalBoxFuture;
use futures::prelude::*;
use log::{debug, info, trace, warn};
use ring::{Ring, TrbData};
use xhci::{
    ExtendedCapability,
    accessor::Mapper,
    extended_capabilities::{
        self,
        usb_legacy_support_capability::{UsbLegacySupport, UsbLegacySupportControlStatus},
    },
    registers::doorbell,
    ring::trb::{self, command, event::CommandCompletion},
};

mod context;
mod event;
mod ring;

use super::Controller;
use crate::{err::*, sleep};

type Registers = xhci::Registers<MemMapper>;
type RegistersExtList = xhci::extended_capabilities::List<MemMapper>;
type SupportedProtocol = xhci::extended_capabilities::XhciSupportedProtocol<MemMapper>;

pub struct Xhci {
    mmio_base: NonNull<u8>,
    data: Option<Data>,
}

impl Xhci {
    pub fn new(mmio_base: NonNull<u8>) -> Self {
        Self {
            mmio_base,
            data: None,
        }
    }

    fn regs(&self) -> Registers {
        let mapper = MemMapper {};
        unsafe { Registers::new(self.mmio_base.as_ptr() as usize, mapper) }
    }

    async fn chip_hardware_reset(&mut self) -> Result {
        debug!("Reset begin ...");
        let mut regs = self.regs();
        regs.operational.usbcmd.update_volatile(|c| {
            c.clear_run_stop();
        });

        while !regs.operational.usbsts.read_volatile().hc_halted() {
            sleep(Duration::from_millis(10)).await;
        }

        debug!("Halted");
        let o = &mut regs.operational;
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
        let mut regs = self.regs();
        let max_slots = regs
            .capability
            .hcsparams1
            .read_volatile()
            .number_of_device_slots();

        regs.operational.config.update_volatile(|r| {
            r.set_max_device_slots_enabled(max_slots);
        });

        debug!("Max device slots: {}", max_slots);

        max_slots
    }

    fn setup_dcbaap(&mut self) -> Result {
        let dcbaa_addr = self.data()?.dev_list.dcbaa.bus_addr();
        debug!("DCBAAP: {:X}", dcbaa_addr);
        self.regs().operational.dcbaap.update_volatile(|r| {
            r.set(dcbaa_addr);
        });

        Ok(())
    }

    fn set_cmd_ring(&mut self) -> Result {
        let crcr = self.data()?.cmd.trbs.bus_addr();
        let cycle = self.data()?.cmd.cycle;

        debug!("CRCR: {:X}", crcr);
        self.regs().operational.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(crcr);
            if cycle {
                r.set_ring_cycle_state();
            } else {
                r.clear_ring_cycle_state();
            }
        });

        Ok(())
    }

    fn init_irq(&mut self) -> Result {
        debug!("Disable interrupts");
        let mut regs = self.regs();

        regs.operational.usbcmd.update_volatile(|r| {
            r.clear_interrupter_enable();
        });

        let erstz = self.data()?.event.len();
        let erdp = self.data()?.event.erdp();
        let erstba = self.data()?.event.erstba();

        {
            let mut ir0 = regs.interrupter_register_set.interrupter_mut(0);

            debug!("ERDP: {:x}", erdp);

            ir0.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
                r.set_dequeue_erst_segment_index(0);
                r.clear_event_handler_busy();
            });

            debug!("ERSTZ: {:x}", erstz);
            ir0.erstsz.update_volatile(|r| r.set(erstz as _));
            debug!("ERSTBA: {:X}", erstba);
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
            regs.interrupter_register_set
                .interrupter_mut(0)
                .iman
                .update_volatile(|im| {
                    im.set_interrupt_enable();
                    im.clear_interrupt_pending();
                });
        }

        /* Set the HCD state before we enable the irqs */
        regs.operational.usbcmd.update_volatile(|r| {
            r.set_interrupter_enable();
            r.set_host_system_error_enable();
            r.set_enable_wrap_event();
        });
        Ok(())
    }

    fn setup_scratchpads(&mut self) -> Result {
        let scratchpad_buf_arr = {
            let buf_count = {
                let count = self
                    .regs()
                    .capability
                    .hcsparams2
                    .read_volatile()
                    .max_scratchpad_buffers();
                debug!("Scratch buf count: {}", count);
                count
            };
            if buf_count == 0 {
                return Ok(());
            }
            let scratchpad_buf_arr = ScratchpadBufferArray::new(buf_count as _)?;

            let bus_addr = scratchpad_buf_arr.bus_addr();

            self.data()?.dev_list.dcbaa.set(0, bus_addr);

            debug!("Setting up {} scratchpads, at {:#0x}", buf_count, bus_addr);
            scratchpad_buf_arr
        };

        self.data()?.scratchpad_buf_arr = Some(scratchpad_buf_arr);

        Ok(())
    }

    async fn start(&mut self) -> Result {
        let mut regs = self.regs();
        debug!("Start run");

        regs.operational.usbcmd.update_volatile(|r| {
            r.set_run_stop();
        });

        while regs.operational.usbsts.read_volatile().hc_halted() {
            sleep(Duration::from_millis(10)).await;
        }

        info!("Running");

        regs.doorbell
            .write_volatile_at(0, doorbell::Register::default());

        Ok(())
    }

    async fn post_cmd(&mut self, trb: command::Allowed) -> Result {
        let trb_addr = self.data()?.cmd.enque_command(trb);

        self.regs()
            .doorbell
            .write_volatile_at(0, doorbell::Register::default());

        let res = self.data()?.event.wait_result(trb_addr).await;

        if let trb::event::Allowed::CommandCompletion(c) = res {
        } else {
            panic!("Invalid event type")
        }

        Ok(())
    }

    fn extended_capabilities(&self) -> Vec<ExtendedCapability<MemMapper>> {
        let hccparams1 = self.regs().capability.hccparams1.read_volatile();
        let mapper = MemMapper {};
        let mut out = Vec::new();
        let mut l = match unsafe {
            extended_capabilities::List::new(self.mmio_base.as_ptr() as usize, hccparams1, mapper)
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
            match cap {
                ExtendedCapability::UsbLegacySupport(usb_legacy_support) => {
                    self.legacy_init(usb_legacy_support).await?;
                }
                ExtendedCapability::XhciSupportedProtocol(xhci_supported_protocol) => {}
                ExtendedCapability::HciExtendedPowerManagementCapability(generic) => {}
                ExtendedCapability::XhciMessageInterrupt(xhci_message_interrupt) => {}
                ExtendedCapability::XhciLocalMemory(xhci_local_memory) => {}
                ExtendedCapability::Debug(debug) => {}
                ExtendedCapability::XhciExtendedMessageInterrupt(generic) => {}
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

    fn data(&mut self) -> Result<&mut Data> {
        self.data.as_mut().ok_or(USBError::NotInitialized)
    }
}

struct Data {
    dev_list: context::DeviceContextList,
    cmd: Ring,
    event: event::EventRing,
    scratchpad_buf_arr: Option<ScratchpadBufferArray>,
}

impl Data {
    fn new(max_slots: usize) -> Result<Self> {
        let cmd = Ring::new_with_len(
            0x1000 / size_of::<TrbData>(),
            true,
            dma_api::Direction::Bidirectional,
        )?;
        let event = event::EventRing::new(&cmd)?;

        Ok(Self {
            dev_list: context::DeviceContextList::new(max_slots)?,
            cmd,
            event,
            scratchpad_buf_arr: None,
        })
    }
}

impl Controller for Xhci {
    fn init(&mut self) -> LocalBoxFuture<'_, Result> {
        async {
            self.init_ext_caps().await?;
            self.chip_hardware_reset().await?;
            let max_slots = self.setup_max_device_slots();
            self.data = Some(Data::new(max_slots as _)?);
            self.setup_dcbaap()?;
            self.set_cmd_ring()?;
            self.init_irq()?;
            self.setup_scratchpads()?;
            self.start().await?;

            Ok(())
        }
        .boxed_local()
    }

    fn test_cmd(&mut self) -> LocalBoxFuture<'_, Result> {
        async {
            self.post_cmd(command::Allowed::Noop(command::Noop::new()))
                .await?;
            Ok(())
        }
        .boxed_local()
    }

    fn handle_irq(&mut self) {
        let mut sts = self.regs().operational.usbsts.read_volatile();
        if sts.event_interrupt() {
            let erdp = {
                let event = &mut self.data().unwrap().event;
                event.clean_events();
                event.erdp()
            };
            {
                let mut regs = self.regs();
                let mut irq = regs.interrupter_register_set.interrupter_mut(0);

                irq.erdp.update_volatile(|r| {
                    r.set_event_ring_dequeue_pointer(erdp);
                    r.clear_event_handler_busy();
                });

                irq.iman.update_volatile(|r| {
                    r.clear_interrupt_pending();
                });
            }

            sts.clear_event_interrupt();
        }
        if sts.port_change_detect() {
            debug!("Port Change Detected");

            sts.clear_port_change_detect();
        }

        if sts.host_system_error() {
            debug!("Host System Error");
            sts.clear_host_system_error();
        }

        self.regs().operational.usbsts.write_volatile(sts);
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
