use alloc::collections::BTreeMap;

use alloc::{sync::Arc, vec::Vec};

use futures::{FutureExt, future::BoxFuture};
use mbarrier::mb;
use spin::Mutex;
use usb_if::{
    descriptor::{
        ConfigurationDescriptor, DescriptorType, DeviceDescriptor, EndpointDescriptor, EndpointType,
    },
    host::{ControlSetup, USBError},
    transfer::{Recipient, RequestType},
};
use xhci::ring::trb::command;

use crate::backend::xhci::cmd::CommandRing;
use crate::{
    Xhci,
    backend::{
        Dci, PortId,
        ty::{
            DeviceInfoOp, DeviceOp,
            ep::{EndpointBase, EndpointControl},
        },
        xhci::{
            SlotId, append_port_to_route_string,
            context::ContextData,
            endpoint::{Endpoint, EndpointDescriptorExt},
            parse_default_max_packet_size_from_port_speed,
            reg::SlotBell,
            transfer::TransferResultHandler,
        },
    },
    err::Result,
};

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    slot_id: SlotId,
    desc: DeviceDescriptor,
    config_desc: Vec<ConfigurationDescriptor>,
}

impl DeviceInfo {
    pub fn new(
        slot_id: SlotId,
        desc: DeviceDescriptor,
        config_desc: &[ConfigurationDescriptor],
    ) -> Self {
        Self {
            slot_id,
            desc,
            config_desc: config_desc.to_vec(),
        }
    }

    pub fn slot_id(&self) -> SlotId {
        self.slot_id
    }
}

impl DeviceInfoOp for DeviceInfo {
    fn backend_name(&self) -> &str {
        "xhci"
    }

    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor] {
        &self.config_desc
    }
}

pub struct Device {
    id: SlotId,
    port_id: PortId,
    ctx: ContextData,
    desc: DeviceDescriptor,
    ctrl_ep: Option<EndpointControl>,
    transfer_result_handler: TransferResultHandler,
    bell: Arc<Mutex<SlotBell>>,
    dma_mask: usize,
    current_config_value: Option<u8>,
    config_desc: Vec<ConfigurationDescriptor>,
    port_speed: u8,
    eps: BTreeMap<Dci, EndpointBase>,
    cmd: CommandRing,
}

impl Device {
    pub(crate) async fn new(host: &mut Xhci, port: PortId) -> Result<Self> {
        let slot_id = host.device_slot_assignment().await?;
        debug!("Slot {slot_id} assigned");
        let is_64 = host.is_64bit_ctx();
        debug!(
            "Creating new context for slot {slot_id}, {}",
            if is_64 { "64-bit" } else { "32-bit" }
        );
        let dma_mask = host.dma_mask;
        let ctx = host.dev_mut()?.new_ctx(slot_id, is_64, dma_mask)?;
        let bell = host.new_slot_bell(slot_id);
        let bell = Arc::new(Mutex::new(bell));
        let port_speed = host.port_speed(port);
        let desc = unsafe { core::mem::zeroed() };

        Ok(Self {
            id: slot_id,
            port_id: port,
            ctx,
            bell,
            ctrl_ep: None,
            desc,
            dma_mask,
            transfer_result_handler: host.transfer_result_handler.clone(),
            current_config_value: None,
            config_desc: vec![],
            port_speed,
            eps: BTreeMap::new(),
            cmd: host.cmd.clone(),
        })
    }

    pub fn slot_id(&self) -> SlotId {
        self.id
    }

    fn new_ep(&mut self, dci: Dci) -> Result<Endpoint> {
        let ep = Endpoint::new(dci, self.dma_mask, self.bell.clone())?;
        self.transfer_result_handler
            .register_queue(self.id.as_u8(), dci.as_u8(), ep.ring());

        Ok(ep)
    }

    pub fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }

    pub fn configuration_descriptors(&self) -> &[ConfigurationDescriptor] {
        &self.config_desc
    }

    pub(crate) async fn init(&mut self, host: &mut Xhci) -> Result {
        let ep = self.new_ep(Dci::CTRL)?;
        self.ctrl_ep = Some(EndpointControl::new(ep));
        self.address(host).await?;
        // self.dump_device_out();
        let max_packet_size = self.control_max_packet_size().await?;
        trace!("Max packet size: {max_packet_size}");
        self.get_configuration().await?;
        self.read_descriptor().await?;

        for i in 0..self.desc.num_configurations {
            let config_desc = self.ep_ctrl().get_configuration_descriptor(i).await?;
            self.config_desc.push(config_desc);
        }

        Ok(())
    }

    async fn address(&mut self, host: &mut Xhci) -> Result {
        trace!("Addressing device with ID: {}", self.id.as_u8());
        let port_speed = host.port_speed(self.port_id);
        let max_packet_size = parse_default_max_packet_size_from_port_speed(port_speed);

        let route_string = append_port_to_route_string(0, 0);

        let ctrl_ring_addr = self.ep_ctrl().raw.as_raw_mut::<Endpoint>().bus_addr();
        // ctrl dci
        let dci = 1;
        trace!(
            "ctrl ring: {ctrl_ring_addr:x?}, port speed: {port_speed}, max packet size: {max_packet_size}, route string: {route_string}"
        );

        // let ring_cycle_bit = self.ctrl_ep.cycle;

        // 1. Allocate an Input Context data structure (6.2.5) and initialize all fields to
        // ‘0’.
        self.ctx.with_empty_input(|input| {
            let control_context = input.control_mut();
            // Initialize the Input Control Context (6.2.5.1) of the Input Context by
            // setting the A0 and A1 flags to ‘1’. These flags indicate that the Slot
            // Context and the Endpoint 0 Context of the Input Context are affected by
            // the command.
            control_context.set_add_context_flag(0);
            control_context.set_add_context_flag(1);
            for i in 2..32 {
                control_context.clear_drop_context_flag(i);
            }

            // Initialize the Input Slot Context data structure (6.2.2).
            // • Root Hub Port Number = Topology defined.
            // • Route String = Topology defined. Refer to section 8.9 in the USB3 spec. Note
            // that the Route String does not include the Root Hub Port Number.
            // • Context Entries = 1.
            let slot_context = input.device_mut().slot_mut();
            slot_context.clear_multi_tt();
            slot_context.clear_hub();
            slot_context.set_route_string(route_string); // for now, not support more hub ,so hardcode as 0.//TODO: generate route string
            slot_context.set_context_entries(1);
            slot_context.set_max_exit_latency(0);
            slot_context.set_root_hub_port_number(self.port_id.raw() as _); //todo: to use port number
            slot_context.set_number_of_ports(0);
            slot_context.set_parent_hub_slot_id(0);
            slot_context.set_tt_think_time(0);
            slot_context.set_interrupter_target(0);
            slot_context.set_speed(port_speed);

            // Initialize the Input default control Endpoint 0 Context (6.2.3).
            let endpoint_0 = input.device_mut().endpoint_mut(dci);
            // • EP Type = Control.
            endpoint_0.set_endpoint_type(xhci::context::EndpointType::Control);
            // • Max Packet Size = The default maximum packet size for the Default Control Endpoint,
            //   as function of the PORTSC Port Speed field.
            endpoint_0.set_max_packet_size(max_packet_size);
            // • Max Burst Size = 0.
            endpoint_0.set_max_burst_size(0);
            // • TR Dequeue Pointer = Start address of first segment of the Default Control
            //   Endpoint Transfer Ring.
            endpoint_0.set_tr_dequeue_pointer(ctrl_ring_addr.raw());
            // • Dequeue Cycle State (DCS) = 1. Reflects Cycle bit state for valid TRBs written
            //   by software.
            // if ring_cycle_bit {
            endpoint_0.set_dequeue_cycle_state();
            // } else {
            //     endpoint_0.clear_dequeue_cycle_state();
            // }
            // • Interval = 0.
            endpoint_0.set_interval(0);
            // • Max Primary Streams (MaxPStreams) = 0.
            endpoint_0.set_max_primary_streams(0);
            // • Mult = 0.
            endpoint_0.set_mult(0);
            // • Error Count (CErr) = 3.
            endpoint_0.set_error_count(3);
        });

        mb();

        let input_bus_addr = self.ctx.input_bus_addr();
        trace!("Input context bus address: {input_bus_addr:#x?}");
        let result = host
            .cmd_request(command::Allowed::AddressDevice(
                *command::AddressDevice::new()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(input_bus_addr),
            ))
            .await?;

        debug!("Address slot ok {result:x?}");

        Ok(())
    }

    async fn read_descriptor(&mut self) -> Result<()> {
        self.desc = self.ep_ctrl().get_device_descriptor().await?;
        Ok(())
    }

    async fn control_max_packet_size(&mut self) -> Result<u16> {
        trace!("control_fetch_control_point_packet_size");

        let mut data = alloc::vec![0u8; 8];

        self.ep_ctrl()
            .get_descriptor(DescriptorType::DEVICE, 0, 0, &mut data)
            .await?;

        // USB 设备描述符的 bMaxPacketSize0 字段（偏移 7）
        // 对于控制端点，这是直接的字节数值，不需要解码
        let packet_size = data
            .get(7) // bMaxPacketSize0 在设备描述符的第8个字节（索引7）
            .map(|&len| if len == 0 { 8u8 } else { len })
            .unwrap_or(8);

        trace!("data@{:p}: {data:?}", data.as_ptr());
        trace!("Device descriptor bMaxPacketSize0: {packet_size} bytes");

        Ok(packet_size as _)
    }

    async fn get_configuration(&mut self) -> Result<u8> {
        let val = self.ep_ctrl().get_configuration().await?;
        self.current_config_value = Some(val);
        Ok(val)
    }

    async fn _set_configuration(&mut self, configuration_value: u8) -> Result {
        self.ep_ctrl()
            .set_configuration(configuration_value)
            .await?;

        self.current_config_value = Some(configuration_value);

        self.ctx.with_input(|input| {
            let c = input.control_mut();
            c.set_configuration_value(configuration_value);
        });

        debug!("Device configuration set to {configuration_value}");
        Ok(())
    }

    async fn _claim_interface(&mut self, interface: u8, alternate: u8) -> Result {
        self.ctx.with_input(|input| {
            let c = input.control_mut();
            c.set_interface_number(interface);
            c.set_alternate_setting(alternate);
        });

        self.ep_ctrl()
            .control_out(
                ControlSetup {
                    request_type: RequestType::Standard,
                    recipient: Recipient::Interface,
                    request: usb_if::transfer::Request::SetInterface,
                    value: alternate as _, // alternate setting goes in value
                    index: interface as _, // interface number goes in index
                },
                &[],
            )
            .await?;
        self.setup_all_endpoints(interface, alternate).await?;
        debug!("Interface {interface} set successfully");
        Ok(())
    }

    async fn setup_all_endpoints(&mut self, interface: u8, alternate: u8) -> Result {
        let mut max_dci = 1;
        self.ctx.input_perper_modify();
        self.eps.clear();

        for desc in self
            .find_interface_endpoints(interface, alternate)?
            .to_vec()
        {
            let dci = desc.dci();
            if dci > max_dci {
                max_dci = dci;
            }
            let ep_raw = self.new_ep(dci.into())?;
            let ring_addr = ep_raw.bus_addr();
            self.eps.insert(dci.into(), EndpointBase::new(ep_raw));

            let xhci_interval =
                self.calculate_xhci_interval(desc.interval, desc.transfer_type, desc.interval);

            self.ctx.with_input(|input| {
                let control_context = input.control_mut();

                control_context.set_add_context_flag(dci as _);

                debug!(
                    "init ep addr {:#x}  dci {dci} {:?}",
                    desc.address, desc.transfer_type
                );

                let ep_mut = input.device_mut().endpoint_mut(dci as _);

                debug!(
                    "Set XHCI interval: {} (original bInterval: {})",
                    xhci_interval, desc.interval
                );
                ep_mut.set_interval(xhci_interval);
                ep_mut.set_endpoint_type(desc.endpoint_type());
                ep_mut.set_tr_dequeue_pointer(ring_addr.raw());
                ep_mut.set_max_packet_size(desc.max_packet_size);
                ep_mut.set_error_count(3);
                ep_mut.set_dequeue_cycle_state();

                match desc.transfer_type {
                    EndpointType::Isochronous | EndpointType::Interrupt => {
                        //init for isoch/interrupt
                        ep_mut.set_max_packet_size(desc.max_packet_size & 0x7ff); //refer xhci page 162
                        ep_mut.set_max_burst_size(
                            ((desc.max_packet_size & 0x1800) >> 11).try_into().unwrap(),
                        );
                        ep_mut.set_mult(0); //always 0 for interrupt
                        ep_mut.set_max_endpoint_service_time_interval_payload_low(4);
                    }
                    _ => {}
                }

                if let EndpointType::Isochronous = desc.transfer_type {
                    ep_mut.set_error_count(0);
                }
            });
        }

        self.ctx.with_input(|input| {
            input
                .device_mut()
                .slot_mut()
                .set_context_entries(max_dci + 1);
        });
        mb();

        let _result = self
            .cmd
            .cmd_request(command::Allowed::ConfigureEndpoint(
                *command::ConfigureEndpoint::default()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(self.ctx.input_bus_addr()),
            ))
            .await?;

        Ok(())
    }

    fn find_interface_endpoints(
        &self,
        interface: u8,
        alternate: u8,
    ) -> Result<&[EndpointDescriptor]> {
        for config in &self.config_desc {
            for iface in &config.interfaces {
                if iface.interface_number == interface {
                    for alt in &iface.alt_settings {
                        if alt.alternate_setting == alternate {
                            return Ok(&alt.endpoints);
                        }
                    }
                }
            }
        }
        Err(USBError::NotFound)
    }

    /// 根据 XHCI 规范计算端点的 interval 值
    /// 参考 xHCI 规范第 6.2.3.6 节
    fn calculate_xhci_interval(
        &self,
        binterval: u8,
        transfer_type: EndpointType,
        default: u8,
    ) -> u8 {
        match transfer_type {
            EndpointType::Isochronous => {
                match self.port_speed {
                    2..=5 => {
                        // HighSpeed, SuperSpeed, SuperSpeedPlus ISO 端点
                        // Interval = max(1, min(16, bInterval))
                        let interval = binterval.clamp(1, 16);
                        info!(
                            "ISO endpoint HS/SS: bInterval={} -> XHCI interval={}",
                            binterval, interval
                        );
                        interval
                    }
                    _ => {
                        // FullSpeed/LowSpeed ISO 端点
                        // Interval = max(1, min(16, floor(log2(bInterval)) + 3))
                        if binterval == 0 {
                            1
                        } else {
                            // 计算 floor(log2(bInterval))
                            let log2_binterval = 31 - (binterval as u32).leading_zeros() as u8 - 1;
                            let interval = (log2_binterval + 3).clamp(1, 16);
                            info!(
                                "ISO endpoint FS/LS: bInterval={} -> log2={} -> XHCI interval={}",
                                binterval, log2_binterval, interval
                            );
                            interval
                        }
                    }
                }
            }
            EndpointType::Interrupt => {
                match self.port_speed {
                    2..=5 => {
                        // HighSpeed, SuperSpeed, SuperSpeedPlus 中断端点
                        // Interval = max(1, min(16, bInterval))
                        let interval = binterval.clamp(1, 16);
                        info!(
                            "INT endpoint HS/SS: bInterval={} -> XHCI interval={}",
                            binterval, interval
                        );
                        interval
                    }
                    _ => {
                        // FullSpeed/LowSpeed 中断端点
                        // Interval = max(1, min(16, floor(log2(bInterval)) + 3))
                        if binterval == 0 {
                            1
                        } else {
                            // 计算 floor(log2(bInterval))
                            let log2_binterval = 31 - (binterval as u32).leading_zeros() as u8 - 1;
                            let interval = (log2_binterval + 3).clamp(1, 16);
                            info!(
                                "INT endpoint FS/LS: bInterval={} -> log2={} -> XHCI interval={}",
                                binterval, log2_binterval, interval
                            );
                            interval
                        }
                    }
                }
            }
            _ => {
                // 控制和批量端点不使用 interval
                default
            }
        }
    }
}

impl DeviceOp for Device {
    fn backend_name(&self) -> &str {
        "xhci"
    }

    fn parent_port_id(&self) -> Option<u8> {
        Some(self.port_id.raw() as u8)
    }

    fn descriptor(&self) -> &DeviceDescriptor {
        &self.desc
    }
    fn claim_interface<'a>(
        &'a mut self,
        interface: u8,
        alternate: u8,
    ) -> BoxFuture<'a, Result<()>> {
        self._claim_interface(interface, alternate).boxed()
    }
    fn set_configuration<'a>(&'a mut self, configuration_value: u8) -> BoxFuture<'a, Result<()>> {
        self._set_configuration(configuration_value).boxed()
    }

    fn ep_ctrl(&mut self) -> &mut EndpointControl {
        self.ctrl_ep.as_mut().unwrap()
    }

    fn configuration_descriptors(&self) -> &[ConfigurationDescriptor] {
        &self.config_desc
    }

    fn get_endpoint(
        &mut self,
        desc: &usb_if::descriptor::EndpointDescriptor,
    ) -> Result<EndpointBase> {
        let ep = self.eps.remove(&desc.dci().into());
        ep.ok_or(USBError::NotFound)
    }
}
