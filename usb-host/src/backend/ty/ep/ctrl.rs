use core::pin::Pin;

use usb_if::descriptor::{ConfigurationDescriptor, DescriptorType, DeviceDescriptor};
use usb_if::err::TransferError;
use usb_if::host::{ControlSetup, USBError};
use usb_if::transfer::{Recipient, Request, RequestType};

use crate::backend::ty::transfer::{Transfer, TransferKind};

use super::{EndpointBase, EndpointOp};

pub struct EndpointControl {
    pub(crate) raw: EndpointBase,
}

impl EndpointControl {
    pub(crate) fn new(raw: impl EndpointOp) -> Self {
        Self {
            raw: EndpointBase::new(raw),
        }
    }

    pub async fn control_in(
        &mut self,
        param: usb_if::host::ControlSetup,
        buff: &mut [u8],
    ) -> Result<usize, TransferError> {
        let transfer = Transfer::new_in(TransferKind::Control(param), Pin::new(buff));
        let t = self.raw.submit_and_wait(transfer).await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub async fn control_out(
        &mut self,
        param: usb_if::host::ControlSetup,
        buff: &[u8],
    ) -> Result<usize, TransferError> {
        let transfer = Transfer::new_out(TransferKind::Control(param), Pin::new(buff));
        let t = self.raw.submit_and_wait(transfer).await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub async fn set_configuration(
        &mut self,
        configuration_value: u8,
    ) -> Result<(), TransferError> {
        self.control_out(
            ControlSetup {
                request_type: RequestType::Standard,
                recipient: Recipient::Device,
                request: Request::SetConfiguration,
                value: configuration_value as u16,
                index: 0,
            },
            &[],
        )
        .await?;
        Ok(())
    }

    pub async fn get_descriptor(
        &mut self,
        desc_type: DescriptorType,
        desc_index: u8,
        language_id: u16,
        buff: &mut [u8],
    ) -> Result<(), TransferError> {
        self.control_in(
            ControlSetup {
                request_type: RequestType::Standard,
                recipient: Recipient::Device,
                request: Request::GetDescriptor,
                value: ((desc_type.0 as u16) << 8) | desc_index as u16,
                index: language_id,
            },
            buff,
        )
        .await?;
        Ok(())
    }

    pub async fn get_device_descriptor(&mut self) -> Result<DeviceDescriptor, USBError> {
        let mut buff = alloc::vec![0u8; DeviceDescriptor::LEN];
        self.get_descriptor(DescriptorType::DEVICE, 0, 0, &mut buff)
            .await?;
        trace!("data: {buff:?}");
        let desc = DeviceDescriptor::parse(&buff)
            .ok_or(USBError::Other("device descriptor parse err".into()))?;

        Ok(desc)
    }

    pub async fn get_configuration(&mut self) -> Result<u8, TransferError> {
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
        )
        .await?;
        let config_value = buff[0];

        Ok(config_value)
    }

    pub async fn get_configuration_descriptor(
        &mut self,
        index: u8,
    ) -> Result<ConfigurationDescriptor, USBError> {
        let mut header = alloc::vec![0u8; ConfigurationDescriptor::LEN]; // 配置描述符头部固定为9字节
        self.get_descriptor(DescriptorType::CONFIGURATION, index, 0, &mut header)
            .await?;

        let total_length = u16::from_le_bytes(header[2..4].try_into().unwrap()) as usize;
        // 获取完整的配置描述符（包括接口和端点描述符）
        let mut full_data = alloc::vec![0u8; total_length];
        debug!("Reading configuration descriptor for index {index}, total length: {total_length}");
        self.get_descriptor(DescriptorType::CONFIGURATION, index, 0, &mut full_data)
            .await?;

        let parsed_config = ConfigurationDescriptor::parse(&full_data)
            .ok_or(USBError::Other("config descriptor parse err".into()))?;

        Ok(parsed_config)
    }
}

impl From<EndpointBase> for EndpointControl {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}
