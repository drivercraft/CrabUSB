use alloc::vec::Vec;
use core::ptr::NonNull;

use usb_if::{err::TransferError, transfer::Direction};

use crate::backend::ty::{ep::TransferHandle, transfer::TransferKind};

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
        let n = t.transfer_len;
        Ok(n)
    }

    pub async fn submit_and_wait_with_packet_lengths(
        &mut self,
        packets: &mut [u8],
        packet_lengths: &[usize],
    ) -> Result<usize, TransferError> {
        let t = self
            .submit_with_packet_lengths(packets, packet_lengths)?
            .await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub fn submit(
        &mut self,
        packets: &mut [u8],
        num_packets: usize,
    ) -> Result<TransferHandle<'_>, TransferError> {
        let packet_lengths = even_packet_lengths(packets.len(), num_packets);
        self.submit_with_packet_lengths(packets, &packet_lengths)
    }

    pub fn submit_with_packet_lengths(
        &mut self,
        packets: &mut [u8],
        packet_lengths: &[usize],
    ) -> Result<TransferHandle<'_>, TransferError> {
        let buff = if packets.is_empty() {
            None
        } else {
            Some((NonNull::new(packets.as_mut_ptr()).unwrap(), packets.len()))
        };

        let transfer = self.raw.new_transfer(
            TransferKind::Isochronous {
                packet_lengths: packet_lengths.to_vec(),
            },
            Direction::In,
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

    pub async fn submit_and_wait_with_packet_lengths(
        &mut self,
        packets: &[u8],
        packet_lengths: &[usize],
    ) -> Result<usize, TransferError> {
        let t = self
            .submit_with_packet_lengths(packets, packet_lengths)?
            .await?;
        let n = t.transfer_len;
        Ok(n)
    }

    pub fn submit(
        &mut self,
        packets: &[u8],
        num_packets: usize,
    ) -> Result<TransferHandle<'_>, TransferError> {
        let packet_lengths = even_packet_lengths(packets.len(), num_packets);
        self.submit_with_packet_lengths(packets, &packet_lengths)
    }

    pub fn submit_with_packet_lengths(
        &mut self,
        packets: &[u8],
        packet_lengths: &[usize],
    ) -> Result<TransferHandle<'_>, TransferError> {
        let buff = if packets.is_empty() {
            None
        } else {
            Some((
                NonNull::new(packets.as_ptr() as *mut u8).unwrap(),
                packets.len(),
            ))
        };
        let transfer = self.raw.new_transfer(
            TransferKind::Isochronous {
                packet_lengths: packet_lengths.to_vec(),
            },
            Direction::Out,
            buff,
        );
        self.raw.submit(transfer)
    }
}

impl From<EndpointBase> for EndpointIsoOut {
    fn from(raw: EndpointBase) -> Self {
        Self { raw }
    }
}

fn even_packet_lengths(total_len: usize, num_packets: usize) -> Vec<usize> {
    if num_packets == 0 {
        return Vec::new();
    }

    if total_len == 0 {
        return alloc::vec![0; num_packets];
    }

    let packet_size = total_len.div_ceil(num_packets);
    let mut remaining = total_len;
    let mut packet_lengths = Vec::with_capacity(num_packets);
    for _ in 0..num_packets {
        let current = remaining.min(packet_size);
        packet_lengths.push(current);
        remaining = remaining.saturating_sub(current);
    }
    packet_lengths
}
