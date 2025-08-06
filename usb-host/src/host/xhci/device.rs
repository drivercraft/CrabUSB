use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::ops::{Deref, DerefMut};

use futures::FutureExt;
use log::{debug, trace};
use mbarrier::mb;
use spin::Mutex;
use usb_if::{
    descriptor::{
        self, ConfigurationDescriptor, DescriptorType, DeviceDescriptor, EndpointDescriptor,
        InterfaceDescriptor, decode_string_descriptor,
    },
    host::{ControlSetup, ResultTransfer},
    transfer::{Recipient, Request, RequestType},
};
use xhci::{registers::doorbell, ring::trb::command};

use crate::{
    PortId,
    err::USBError,
    host::xhci::{
        append_port_to_route_string,
        context::ContextData,
        def::{Dci, SlotId},
        endpoint::{EndpointControl, EndpointDescriptorExt, EndpointRaw},
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
    pub(crate) ctrl_ep: EndpointControl,
}

pub struct DeviceInfo {
    device: Option<Device>,
}

impl DeviceInfo {
    pub(crate) fn new(device: Device) -> Self {
        Self {
            device: Some(device),
        }
    }
}

impl usb_if::host::DeviceInfo for DeviceInfo {
    fn open(
        &mut self,
    ) -> futures::future::LocalBoxFuture<'_, Result<Box<dyn usb_if::host::Device>, USBError>> {
        async {
            self.device
                .take()
                .ok_or(USBError::Other("Device already opened".into()))
                .map(|d| Box::new(d) as Box<dyn usb_if::host::Device>)
        }
        .boxed_local()
    }

    fn descriptor(
        &self,
    ) -> futures::future::LocalBoxFuture<'_, Result<DeviceDescriptor, USBError>> {
        async { Ok(self.device.as_ref().unwrap().desc.clone().unwrap()) }.boxed_local()
    }

    fn configuration_descriptor(
        &mut self,
        index: u8,
    ) -> futures::future::LocalBoxFuture<'_, Result<ConfigurationDescriptor, USBError>> {
        async move {
            let config = self
                .device
                .as_mut()
                .ok_or(USBError::Other("Device not opened".into()))?
                .read_configuration_descriptor(index)
                .await?;

            let parsed_config = ConfigurationDescriptor::parse(&config)
                .ok_or(USBError::Other("config descriptor parse err".into()))?;
            Ok(parsed_config)
        }
        .boxed_local()
    }
}

impl usb_if::host::Device for Device {
    fn set_configuration(
        &mut self,
        configuration: u8,
    ) -> futures::future::LocalBoxFuture<'_, Result<(), USBError>> {
        trace!("Setting device configuration to {configuration}");
        async move {
            self.control_out(
                ControlSetup {
                    request_type: RequestType::Standard,
                    recipient: Recipient::Device,
                    request: Request::SetConfiguration,
                    value: configuration as u16,
                    index: 0,
                },
                &[],
            )?
            .await?;

            self.current_config_value = Some(configuration);

            self.ctx.with_input(|input| {
                let c = input.control_mut();
                c.set_configuration_value(configuration);
            });

            debug!("Device configuration set to {configuration}");
            Ok(())
        }
        .boxed_local()
    }

    fn get_configuration(&mut self) -> futures::future::LocalBoxFuture<'_, Result<u8, USBError>> {
        async move {
            let mut buff = alloc::vec![0u8; 1];
            self.control_in(
                ControlSetup {
                    request_type: RequestType::Standard,
                    recipient: Recipient::Device,
                    request: Request::GetConfiguration,
                    value: 0,
                    index: 0,
                },
                &mut buff,
            )?
            .await?;
            let config_value = buff[0];
            self.current_config_value = Some(config_value);
            Ok(buff[0])
        }
        .boxed_local()
    }

    fn claim_interface(
        &mut self,
        interface: u8,
        alternate: u8,
    ) -> futures::future::LocalBoxFuture<'_, Result<Box<dyn usb_if::host::Interface>, USBError>>
    {
        async move {
            trace!("Claiming interface {interface}, alternate {alternate}");
            self.set_interface(interface, alternate).await?;
            let ep_map = self.set_interface(interface, alternate).await?;
            let desc = self.find_interface_desc(interface, alternate)?;
            let interface = Interface::new(desc, ep_map, self.ctrl_ep.clone());
            Ok(Box::new(interface) as Box<dyn usb_if::host::Interface>)
        }
        .boxed_local()
    }

    fn string_descriptor(
        &mut self,
        index: u8,
        language_id: u16,
    ) -> futures::future::LocalBoxFuture<'_, Result<String, USBError>> {
        async move {
            let mut data = alloc::vec![0u8; 256];
            self.get_descriptor(DescriptorType::STRING, index, language_id, &mut data)
                .await?;
            let res = decode_string_descriptor(&data).map_err(|e| USBError::Other(e.into()))?;
            Ok(res)
        }
        .boxed_local()
    }
}

impl Device {
    pub(crate) fn new(
        id: SlotId,
        root: &RootHub,
        ctx: *mut ContextData,
        port_id: PortId,
    ) -> Result<Self, USBError> {
        let state = DeviceState::new(id, unsafe { root.reg() }, root.clone());
        let ctrl_ep = EndpointControl::new(EndpointRaw::new(Dci::CTRL, &state)?);

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
        for i in 0..desc.num_configurations {
            let config = self.read_configuration_descriptor(i).await?;
            let parsed_config = ConfigurationDescriptor::parse(&config)
                .ok_or(USBError::Other("config descriptor parse err".into()))?;
            self.config_desc.push(parsed_config);
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

    pub fn control_in<'a>(
        &mut self,
        param: ControlSetup,
        buff: &'a mut [u8],
    ) -> ResultTransfer<'a> {
        self.ctrl_ep.control_in(param, buff)
    }

    pub fn control_out<'a>(&mut self, param: ControlSetup, buff: &'a [u8]) -> ResultTransfer<'a> {
        self.ctrl_ep.control_out(param, buff)
    }

    async fn control_max_packet_size(&mut self) -> Result<u16, USBError> {
        trace!("control_fetch_control_point_packet_size");

        let mut data = alloc::vec![0u8; 8];

        self.get_descriptor(DescriptorType::DEVICE, 0, 0, &mut data)
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
        let mut buff = alloc::vec![0u8; DeviceDescriptor::LEN];
        self.get_descriptor(DescriptorType::DEVICE, 0, 0, &mut buff)
            .await?;
        let desc = DeviceDescriptor::parse(&buff)
            .ok_or(USBError::Other("device descriptor parse err".into()))?;
        self.desc = Some(desc.clone());
        Ok(desc)
    }

    async fn get_descriptor(
        &mut self,
        desc_type: DescriptorType,
        desc_index: u8,
        language_id: u16,
        buff: &mut [u8],
    ) -> Result<(), USBError> {
        self.control_in(
            ControlSetup {
                request_type: RequestType::Standard,
                recipient: Recipient::Device,
                request: Request::GetDescriptor,
                value: ((desc_type.0 as u16) << 8) | desc_index as u16,
                index: language_id,
            },
            buff,
        )?
        .await?;
        Ok(())
    }

    // pub async fn string_descriptor(
    //     &mut self,
    //     index: NonZero<u8>,
    //     language_id: u16,
    // ) -> Result<String, USBError> {
    //     let mut data = alloc::vec![0u8; 256];
    //     self.get_descriptor(DescriptorType::STRING, index.get(), language_id, &mut data)
    //         .await?;
    //     let res = decode_string_descriptor(&data).map_err(|e| USBError::Other(e.into()))?;
    //     Ok(res)
    // }

    async fn read_configuration_descriptor(&mut self, index: u8) -> Result<Vec<u8>, USBError> {
        let mut header = alloc::vec![0u8; ConfigurationDescriptor::LEN]; // 配置描述符头部固定为9字节
        self.get_descriptor(DescriptorType::CONFIGURATION, index, 0, &mut header)
            .await?;

        let total_length = u16::from_le_bytes(header[2..4].try_into().unwrap()) as usize;
        // 获取完整的配置描述符（包括接口和端点描述符）
        let mut full_data = alloc::vec![0u8; total_length];
        debug!("Reading configuration descriptor for index {index}, total length: {total_length}");
        self.get_descriptor(DescriptorType::CONFIGURATION, index, 0, &mut full_data)
            .await?;

        Ok(full_data)
    }

    /// 配置端点（为指定配置设置端点上下文）
    async fn configure_endpoints_internal(
        &mut self,
        endpoints: &[EndpointDescriptor],
    ) -> Result<BTreeMap<Dci, EndpointRaw>, USBError> {
        trace!("Configuring endpoints for interface");
        let _ = self
            .current_config_value
            .ok_or(USBError::Other("Configuration not set".into()))?;
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
                    descriptor::EndpointType::Isochronous | descriptor::EndpointType::Interrupt => {
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

                if let descriptor::EndpointType::Isochronous = ep.transfer_type {
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
            ControlSetup {
                request_type: RequestType::Standard,
                recipient: Recipient::Interface,
                request: usb_if::transfer::Request::SetInterface,
                value: interface as _,
                index: alternate as _,
            },
            &[],
        )?
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
        for config in &self.config_desc {
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
