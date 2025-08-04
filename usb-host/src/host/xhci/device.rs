use core::{
    num::NonZero,
    ops::{Deref, DerefMut},
};

use alloc::{collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use dma_api::DSliceMut;
use log::{debug, trace};
use mbarrier::mb;
use spin::Mutex;
use xhci::{
    registers::doorbell,
    ring::trb::{command, transfer},
};

use crate::{
    IDevice, PortId,
    err::USBError,
    standard::{
        descriptors::{
            self, ConfigurationDescriptor, EndpointDescriptor, InterfaceDescriptor,
            parser::{
                self, DESCRIPTOR_LEN_CONFIGURATION, DESCRIPTOR_LEN_DEVICE,
                DESCRIPTOR_TYPE_CONFIGURATION, DESCRIPTOR_TYPE_DEVICE, DESCRIPTOR_TYPE_STRING,
                DeviceDescriptor, decode_string_descriptor,
            },
        },
        transfer::{
            Direction,
            control::{Control, ControlRaw, ControlType, Recipient, Request, RequestType},
        },
    },
    xhci::{
        append_port_to_route_string,
        context::ContextData,
        def::{Dci, SlotId},
        endpoint::EndpointRaw,
        interface::Interface,
        parse_default_max_packet_size_from_port_speed,
        reg::XhciRegisters,
        root::RootHub,
    },
};

pub struct Device {
    id: SlotId,
    root: RootHub,
    ctx: DeviceContext,
    port_id: PortId,
    desc: Option<DeviceDescriptor>,
    current_config_value: Option<u8>,
    config_desc: Vec<ConfigurationDescriptor>,
    state: DeviceState,
    pub(crate) ctrl_ep: EndpointRaw,
}

impl IDevice for Device {}

impl Device {
    pub(crate) fn new(
        id: SlotId,
        root: &RootHub,
        ctx: *mut ContextData,
        port_id: PortId,
    ) -> Result<Self, USBError> {
        let state = DeviceState::new(id, unsafe { root.reg() }, root.clone());
        let ctrl_ep = EndpointRaw::new(Dci::CTRL, &state)?;

        Ok(Self {
            id,
            root: root.clone(),
            ctx: DeviceContext(ctx),
            port_id,
            desc: None,
            current_config_value: None, // 初始化为未配置状态
            config_desc: Default::default(),
            state,
            ctrl_ep,
        })
    }

    fn ctx(&self) -> &ContextData {
        unsafe { self.ctx.0.as_ref().expect("Device context pointer is null") }
    }

    pub(crate) async fn init(&mut self) -> Result<(), USBError> {
        trace!("Initializing device with ID: {}", self.id.as_u8());
        // Perform initialization logic here
        self.address().await?;
        let max_packet_size = self.control_max_packet_size().await?;

        trace!("Max packet size: {max_packet_size}");
        self.set_ep_packet_size(Dci::CTRL, max_packet_size).await?;

        let desc = self.descriptor().await?;
        for i in 0..desc.num_configurations() {
            let config = self.read_configuration_descriptor(i).await?;
            let parsed_config =
                parser::ConfigurationDescriptor::new(&config).ok_or(USBError::Unknown)?;
            self.config_desc.push(parsed_config.into());
        }

        Ok(())
    }

    async fn address(&mut self) -> Result<(), USBError> {
        trace!("Addressing device with ID: {}", self.id.as_u8());
        let port_speed = self.root.lock().port_speed(self.port_id);
        let max_packet_size = parse_default_max_packet_size_from_port_speed(port_speed);

        let ctrl_ring_addr = self.ctrl_ep.bus_addr();
        // ctrl dci
        let dci = 1;
        trace!(
            "ctrl ring: {ctrl_ring_addr:x?}, port speed: {port_speed}, max packet size: {max_packet_size}"
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
            slot_context.set_route_string(append_port_to_route_string(0, 0)); // for now, not support more hub ,so hardcode as 0.//TODO: generate route string
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

        let input_bus_addr = self.ctx().input_bus_addr();
        trace!("Input context bus address: {input_bus_addr:#x?}");
        let result = self
            .root
            .post_cmd(command::Allowed::AddressDevice(
                *command::AddressDevice::new()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(input_bus_addr),
            ))
            .await?;

        debug!("Address slot ok {result:?}");

        Ok(())
    }

    pub async fn control_in(&mut self, param: Control, buff: &mut [u8]) -> Result<usize, USBError> {
        self.control_transfer(ControlRaw {
            request_type: RequestType {
                direction: Direction::In,
                control_type: param.transfer_type,
                recipient: param.recipient,
            },
            request: param.request,
            index: param.index,
            value: param.value,
            data: if buff.is_empty() {
                None
            } else {
                Some((buff.as_mut_ptr() as usize, buff.len() as _))
            },
        })
        .await
    }

    pub async fn control_out(&mut self, param: Control, buff: &[u8]) -> Result<usize, USBError> {
        self.control_transfer(ControlRaw {
            request_type: RequestType {
                direction: Direction::Out,
                control_type: param.transfer_type,
                recipient: param.recipient,
            },
            request: param.request,
            index: param.index,
            value: param.value,
            data: if buff.is_empty() {
                None
            } else {
                Some((buff.as_ptr() as usize, buff.len() as _))
            },
        })
        .await
    }

    async fn control_transfer(&mut self, urb: ControlRaw) -> Result<usize, USBError> {
        let mut trbs: Vec<transfer::Allowed> = Vec::new();
        let mut setup = transfer::SetupStage::default();
        let mut buff_data = 0;
        let mut buff_len = 0;

        setup
            .set_request_type(urb.request_type.clone().into())
            .set_request(urb.request.into())
            .set_value(urb.value)
            .set_index(urb.index)
            .set_transfer_type(transfer::TransferType::No);

        let mut data = None;

        if let Some((addr, len)) = urb.data {
            buff_data = addr;
            buff_len = len as usize;
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) };

            let dm = DSliceMut::from(data_slice, dma_api::Direction::Bidirectional);

            if matches!(urb.request_type.direction, Direction::Out) {
                dm.confirm_write_all();
            }

            setup
                .set_transfer_type(urb.request_type.direction.into())
                .set_length(len);

            let mut raw_data = transfer::DataStage::default();
            raw_data
                .set_data_buffer_pointer(dm.bus_addr() as _)
                .set_trb_transfer_length(len as _)
                .set_direction(urb.request_type.direction.into());

            data = Some(raw_data)
        }

        let mut status = transfer::StatusStage::default();
        status.set_interrupt_on_completion();

        if matches!(urb.request_type.direction, Direction::In) {
            status.set_direction();
        }

        trbs.push(setup.into());
        if let Some(data) = data {
            trbs.push(data.into());
        }
        trbs.push(status.into());

        self.ctrl_ep
            .enque(
                trbs.into_iter(),
                urb.request_type.direction,
                buff_data,
                buff_len,
            )
            .await
    }

    async fn control_max_packet_size(&mut self) -> Result<u16, USBError> {
        trace!("control_fetch_control_point_packet_size");

        let mut data = alloc::vec![0u8; 8];

        self.get_descriptor(DESCRIPTOR_TYPE_DEVICE, 0, 0, &mut data)
            .await?;

        // USB 设备描述符的 bMaxPacketSize0 字段（偏移 7）
        // 对于控制端点，这是直接的字节数值，不需要解码
        let packet_size = data
            .get(7) // bMaxPacketSize0 在设备描述符的第8个字节（索引7）
            .map(|&len| if len == 0 { 8u8 } else { len })
            .unwrap_or(8);

        trace!("Device descriptor bMaxPacketSize0: {packet_size} bytes");

        Ok(packet_size as _)
    }

    async fn set_ep_packet_size(&mut self, dci: Dci, max_packet_size: u16) -> Result<(), USBError> {
        self.ctx.with_input(|input| {
            let endpoint = input.device_mut().endpoint_mut(dci.as_usize());
            endpoint.set_max_packet_size(max_packet_size);
        });

        mb();

        let _ = self
            .root
            .post_cmd(command::Allowed::EvaluateContext(
                *command::EvaluateContext::default()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(self.ctx().input_bus_addr()),
            ))
            .await?;

        debug!(
            "EvaluateContext success, packet size {max_packet_size:?}{}",
            if max_packet_size == 9 { "(512)" } else { "" }
        );

        Ok(())
    }

    pub async fn descriptor(&mut self) -> Result<DeviceDescriptor, USBError> {
        if let Some(desc) = &self.desc {
            return Ok(desc.clone());
        }
        let mut buff = alloc::vec![0u8; DESCRIPTOR_LEN_DEVICE as usize];
        self.get_descriptor(DESCRIPTOR_TYPE_DEVICE, 0, 0, &mut buff)
            .await?;
        let desc = DeviceDescriptor::new(&buff).ok_or(USBError::Unknown)?;
        self.desc = Some(desc.clone());
        Ok(desc)
    }

    async fn get_descriptor(
        &mut self,
        desc_type: u8,
        desc_index: u8,
        language_id: u16,
        buff: &mut [u8],
    ) -> Result<(), USBError> {
        self.control_in(
            Control {
                request: Request::GetDescriptor,
                index: language_id,
                value: ((desc_type as u16) << 8) | desc_index as u16,
                transfer_type: ControlType::Standard,
                recipient: Recipient::Device,
            },
            buff,
        )
        .await?;
        Ok(())
    }

    pub async fn string_descriptor(
        &mut self,
        index: NonZero<u8>,
        language_id: u16,
    ) -> Result<String, USBError> {
        let mut data = alloc::vec![0u8; 256];
        self.get_descriptor(DESCRIPTOR_TYPE_STRING, index.get(), language_id, &mut data)
            .await?;
        decode_string_descriptor(&data).map_err(|_| USBError::Unknown)
    }

    async fn read_configuration_descriptor(&mut self, index: u8) -> Result<Vec<u8>, USBError> {
        let mut header = alloc::vec![0u8; DESCRIPTOR_LEN_CONFIGURATION as usize]; // 配置描述符头部固定为9字节
        self.get_descriptor(DESCRIPTOR_TYPE_CONFIGURATION, index, 0, &mut header)
            .await?;

        let total_length = u16::from_le_bytes(header[2..4].try_into().unwrap()) as usize;
        // 获取完整的配置描述符（包括接口和端点描述符）
        let mut full_data = alloc::vec![0u8; total_length];
        debug!("Reading configuration descriptor for index {index}, total length: {total_length}");
        self.get_descriptor(DESCRIPTOR_TYPE_CONFIGURATION, index, 0, &mut full_data)
            .await?;

        Ok(full_data)
    }

    pub fn configuration_descriptors(&self) -> &[ConfigurationDescriptor] {
        &self.config_desc
    }

    async fn set_configuration(&mut self, config_value: u8) -> Result<(), USBError> {
        trace!("Setting device configuration to {config_value}");

        self.control_out(
            Control {
                request: Request::SetConfiguration,
                index: 0,
                value: config_value as u16,
                transfer_type: ControlType::Standard,
                recipient: Recipient::Device,
            },
            &[],
        )
        .await?;

        self.current_config_value = Some(config_value);

        self.ctx.with_input(|input| {
            let c = input.control_mut();
            c.set_configuration_value(config_value);
        });

        debug!("Device configuration set to {config_value}");
        Ok(())
    }

    /// 配置端点（为指定配置设置端点上下文）
    async fn configure_endpoints_internal(
        &mut self,
        endpoints: &[EndpointDescriptor],
    ) -> Result<BTreeMap<Dci, EndpointRaw>, USBError> {
        trace!("Configuring endpoints for interface");
        let _ = self
            .current_config_value
            .ok_or(USBError::ConfigurationNotSet)?;
        let ar = self.root.clone();
        let mut root = ar.lock();
        let mut max_dci = 1;
        let mut out: BTreeMap<Dci, EndpointRaw> = Default::default();
        self.ctx.input_perper_modify();

        // 预先计算所有端点的DCI并创建环
        for ep in endpoints {
            let dci = ep.dci();
            if dci > max_dci {
                max_dci = dci;
            }

            let ep_raw = EndpointRaw::new(dci.into(), &self.state)?;
            // let ring = self.ctx_mut().new_ring(dci.into())?;
            root.litsen_transfer(&ep_raw.ring);
            // let ring_addr = ring.bus_addr();
            let ring_addr = ep_raw.bus_addr();
            out.insert(dci.into(), ep_raw);
            trace!(
                "Configuring endpoint: DCI {}, Type: {:?}, Max Packet Size: {}, Interval: {}",
                dci,
                ep.endpoint_type(),
                ep.max_packet_size,
                ep.interval
            );

            self.ctx.with_input(|input| {
                let control_context = input.control_mut();

                control_context.set_add_context_flag(dci as usize);

                debug!(
                    "init ep addr {:#x}  dci {dci} {:?}",
                    ep.address,
                    ep.endpoint_type()
                );

                let ep_mut = input.device_mut().endpoint_mut(dci as _);
                ep_mut.set_interval(ep.interval);
                ep_mut.set_endpoint_type(ep.endpoint_type());
                ep_mut.set_tr_dequeue_pointer(ring_addr.raw());
                ep_mut.set_max_packet_size(ep.max_packet_size);
                ep_mut.set_error_count(3);
                ep_mut.set_dequeue_cycle_state();

                match ep.transfer_type {
                    descriptors::EndpointType::Isochronous
                    | descriptors::EndpointType::Interrupt => {
                        //init for isoch/interrupt
                        ep_mut.set_max_packet_size(ep.max_packet_size & 0x7ff); //refer xhci page 162
                        ep_mut.set_max_burst_size(
                            ((ep.max_packet_size & 0x1800) >> 11).try_into().unwrap(),
                        );
                        ep_mut.set_mult(0); //always 0 for interrupt
                        ep_mut.set_max_endpoint_service_time_interval_payload_low(4);
                    }
                    _ => {}
                }

                if let descriptors::EndpointType::Isochronous = ep.transfer_type {
                    ep_mut.set_error_count(0);
                }
            });
        }
        drop(root);

        self.ctx.with_input(|input| {
            input
                .device_mut()
                .slot_mut()
                .set_context_entries(max_dci + 1);
        });

        mb();

        let _result = self
            .root
            .post_cmd(command::Allowed::ConfigureEndpoint(
                *command::ConfigureEndpoint::default()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(self.ctx().input_bus_addr()),
            ))
            .await?;

        debug!("Endpoints configured successfully");
        Ok(out)
    }

    async fn set_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> Result<BTreeMap<Dci, EndpointRaw>, USBError> {
        trace!("Setting interface {interface}, alternate {alternate}");

        self.ctx.with_input(|input| {
            let c = input.control_mut();
            c.set_interface_number(interface);
            c.set_alternate_setting(alternate);
        });

        self.control_out(
            Control {
                request: Request::SetInterface,
                index: interface as _,
                value: alternate as _,
                transfer_type: ControlType::Standard,
                recipient: Recipient::Interface,
            },
            &[],
        )
        .await?;
        debug!("Interface {interface} set successfully");

        let endpoints = self.find_interface_endpoints(interface, alternate)?;

        self.configure_endpoints_internal(&endpoints).await
    }

    fn find_interface_endpoints(
        &self,
        interface: u8,
        alternate: u8,
    ) -> Result<Vec<EndpointDescriptor>, USBError> {
        for config in self.configuration_descriptors() {
            for iface in &config.interfaces {
                if iface.interface_number == interface {
                    for alt in &iface.alt_settings {
                        if alt.alternate_setting == alternate {
                            return Ok(alt.endpoints.clone());
                        }
                    }
                }
            }
        }
        Err(USBError::NotFound)
    }

    pub async fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> Result<Interface, USBError> {
        trace!("Claiming interface {interface}, alternate {alternate}");
        let config = self.find_interface_config(interface)?;
        self.set_configuration(config.configuration_value).await?;
        let ep_map = self.set_interface(interface, alternate).await?;
        let desc = self.find_interface_desc(interface, alternate)?;
        let interface = Interface::new(desc, ep_map);
        Ok(interface)
    }

    fn find_interface_config(&self, interface: u8) -> Result<&ConfigurationDescriptor, USBError> {
        for config in &self.config_desc {
            for iface in &config.interfaces {
                if iface.interface_number == interface {
                    return Ok(config);
                }
            }
        }
        Err(USBError::NotFound)
    }

    fn find_interface_desc(
        &self,
        interface: u8,
        alternate: u8,
    ) -> Result<InterfaceDescriptor, USBError> {
        for config in &self.config_desc {
            for iface in &config.interfaces {
                if iface.interface_number == interface {
                    for alt in &iface.alt_settings {
                        if alt.alternate_setting == alternate {
                            return Ok(alt.clone());
                        }
                    }
                }
            }
        }
        Err(USBError::NotFound)
    }
}

#[derive(Clone)]
pub(crate) struct DeviceState {
    id: SlotId,
    inner: Arc<Mutex<DeviceStateInner>>,
    pub root: RootHub,
}

unsafe impl Send for DeviceState {}
unsafe impl Sync for DeviceState {}

impl DeviceState {
    pub fn new(id: SlotId, regs: XhciRegisters, root: RootHub) -> Self {
        Self {
            id,
            inner: Arc::new(Mutex::new(DeviceStateInner { regs })),
            root,
        }
    }

    pub fn doorbell(&self, bell: doorbell::Register) {
        self.inner
            .lock()
            .regs
            .doorbell
            .write_volatile_at(self.id.as_usize(), bell);
    }
}

struct DeviceStateInner {
    regs: XhciRegisters,
}
struct DeviceContext(*mut ContextData);
unsafe impl Send for DeviceContext {}
unsafe impl Sync for DeviceContext {}

impl Deref for DeviceContext {
    type Target = ContextData;

    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref().expect("Device context pointer is null") }
    }
}

impl DerefMut for DeviceContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut().expect("Device context pointer is null") }
    }
}
