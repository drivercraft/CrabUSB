use core::{pin::Pin, ptr::NonNull};

use usb_if::err::TransferError;

use crate::backend::ty::{
    ep::TransferHandle,
    transfer::{Transfer, TransferKind},
};

use super::EndpointBase;

pub struct EndpointIsoIn {
    pub(crate) raw: EndpointBase,
}

impl EndpointIsoIn {
    pub async fn submit_and_wait(
        &mut self,
        packets: &mut [u8],
        num_packets: usize,
    ) -> Result<usize, TransferError> {
        let t = self.submit(packets, num_packets)?.await?;
        let n = t.transfer_len();
        Ok(n)
    }

    pub fn submit(
        &mut self,
        packets: &mut [u8],
        num_packets: usize,
    ) -> Result<TransferHandle<'_>, TransferError> {
        // let transfer = Transfer::new_in(
        //     self.raw.kernel(),
        //     TransferKind::Isochronous {
        //         num_pkgs: num_packets,
        //     },
        //     Pin::new(packets),
        // );

        let buff = if packets.is_empty() {
            None
        } else {
            Some((NonNull::new(packets.as_mut_ptr()).unwrap(), packets.len()))
        };

        let transfer = self.raw.new_transfer(
            TransferKind::Isochronous {
                num_pkgs: num_packets,
            },
            buff,
        );

        self.raw.submit(transfer)
    }
}

impl From<EndpointBase> for EndpointIsoIn {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}

pub struct EndpointIsoOut {
    pub(crate) raw: EndpointBase,
}

impl EndpointIsoOut {
    pub async fn submit_and_wait(
        &mut self,
        packets: &[u8],
        num_packets: usize,
    ) -> Result<usize, TransferError> {
        let t = self.submit(packets, num_packets)?.await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub fn submit(
        &mut self,
        packets: &[u8],
        num_packets: usize,
    ) -> Result<TransferHandle<'_>, TransferError> {
        let transfer = Transfer::new_out(
            self.raw.kernel(),
            TransferKind::Isochronous {
                num_pkgs: num_packets,
            },
            Pin::new(packets),
        );
        self.raw.submit(transfer)
    }
}

impl From<EndpointBase> for EndpointIsoOut {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}
