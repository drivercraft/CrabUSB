use core::pin::Pin;

use usb_if::descriptor::{DescriptorType, DeviceDescriptor};
use usb_if::err::TransferError;
use usb_if::host::{ControlSetup, USBError};
use usb_if::transfer::{Recipient, Request, RequestType};

use crate::backend::ty::transfer::{Transfer, TransferKind};

use super::{EndpointBase, EndpointOp};

pub struct EndpointControl<T: EndpointOp> {
    pub(crate) raw: EndpointBase<T>,
}

impl<T: EndpointOp> EndpointControl<T> {
    pub fn new(raw: T) -> Self {
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
        let t = self.raw.request(transfer).await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub async fn control_out(
        &mut self,
        param: usb_if::host::ControlSetup,
        buff: &[u8],
    ) -> Result<usize, TransferError> {
        let transfer = Transfer::new_out(TransferKind::Control(param), Pin::new(buff));
        let t = self.raw.request(transfer).await?;
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
}
