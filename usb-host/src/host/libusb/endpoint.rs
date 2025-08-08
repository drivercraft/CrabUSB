use core::ptr::null_mut;

use libusb1_sys::{
    libusb_device_handle, libusb_fill_bulk_transfer, libusb_fill_interrupt_transfer,
    libusb_fill_iso_transfer, libusb_get_iso_packet_buffer_simple, libusb_transfer,
};
use log::trace;
use usb_if::host::{
    EndpintIsoIn, EndpintIsoOut, EndpointBulkIn, EndpointBulkOut, EndpointInterruptIn,
    EndpointInterruptOut,
};

use crate::host::libusb::queue::Queue;

pub struct EndpointImpl {
    dev: *mut libusb_device_handle,
    queue: Queue,
    address: u8,
}

impl EndpointImpl {
    pub fn new(raw: *mut libusb_device_handle, address: u8, queue_size: usize) -> Box<Self> {
        Box::new(Self {
            dev: raw,
            queue: Queue::new(queue_size),
            address,
        })
    }
}

unsafe impl Send for EndpointImpl {}

impl usb_if::host::TEndpoint for EndpointImpl {}

impl EndpointBulkIn for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_bulk_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_mut_ptr(),
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpointBulkOut for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_bulk_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_ptr() as *mut u8,
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpointInterruptIn for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_interrupt_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_mut_ptr(),
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpointInterruptOut for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_interrupt_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_ptr() as *mut u8,
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpintIsoIn for EndpointImpl {
    fn submit<'a>(
        &mut self,
        data: &'a mut [u8],
        num_iso_packets: usize,
    ) -> usb_if::host::ResultTransfer<'a> {
        self.queue
            .submit_iso(num_iso_packets, |transfer, on_ready| unsafe {
                on_ready.on_ready = iso_in_on_ready;
                on_ready.param2 = data.as_mut_ptr() as *mut ();
                on_ready.param3 = data.len() as *mut ();

                libusb_fill_iso_transfer(
                    transfer.ptr,
                    self.dev,
                    self.address,
                    data.as_mut_ptr(),
                    data.len() as _,
                    num_iso_packets as _,
                    transfer_callback,
                    null_mut(),
                    1000,
                );

                // 设置每个 ISO packet 的长度，防止溢出
                let packet_size = data.len() / num_iso_packets;
                for i in 0..num_iso_packets {
                    let packet = &mut *(*transfer.ptr).iso_packet_desc.as_mut_ptr().add(i);
                    packet.length = packet_size as u32;
                }
            })
    }
}

fn iso_in_on_ready(trans: *mut (), dest: *mut (), len: *mut ()) {
    unsafe {
        // Handle transfer completion
        let dest = dest as *mut u8;
        let _len = len as usize; // 总缓冲区长度，暂时未使用
        let transfer = &mut *(trans as *mut libusb_transfer);

        let mut current_offset = 0;
        for packet_id in 0..transfer.num_iso_packets as usize {
            let packet = &*transfer.iso_packet_desc.as_mut_ptr().add(packet_id);
            trace!(
                "ISO packet {}: status = {}, length = {}, actual_length = {}",
                packet_id, packet.status, packet.length, packet.actual_length
            );

            match packet.status {
                libusb1_sys::constants::LIBUSB_TRANSFER_COMPLETED => {
                    // 处理成功完成的包
                    let packet_len = packet.actual_length as usize;
                    
                }
                libusb1_sys::constants::LIBUSB_TRANSFER_OVERFLOW => {
                    log::error!(
                        "ISO packet {packet_id} overflow: expected {} bytes, device sent more data",
                        packet.length
                    );
                }
                libusb1_sys::constants::LIBUSB_TRANSFER_ERROR => {
                    log::error!("ISO packet {packet_id} error");
                }
                libusb1_sys::constants::LIBUSB_TRANSFER_TIMED_OUT => {
                    log::warn!("ISO packet {packet_id} timed out");
                }
                libusb1_sys::constants::LIBUSB_TRANSFER_STALL => {
                    log::warn!("ISO packet {packet_id} stalled");
                }
                libusb1_sys::constants::LIBUSB_TRANSFER_NO_DEVICE => {
                    log::error!("ISO packet {packet_id}: device disconnected");
                }
                _ => {
                    log::warn!("ISO packet {packet_id} unknown status: {}", packet.status);
                }
            }
        }
    }
}

impl EndpintIsoOut for EndpointImpl {
    fn submit<'a>(
        &mut self,
        data: &'a [u8],
        num_iso_packets: usize,
    ) -> usb_if::host::ResultTransfer<'a> {
        self.queue
            .submit_iso(num_iso_packets, |transfer, _| unsafe {
                libusb_fill_iso_transfer(
                    transfer.ptr,
                    self.dev,
                    self.address,
                    data.as_ptr() as *mut u8,
                    data.len() as _,
                    num_iso_packets as _,
                    transfer_callback,
                    null_mut(),
                    1000,
                );

                // 设置每个 ISO packet 的长度
                let packet_size = data.len() / num_iso_packets;
                for i in 0..num_iso_packets {
                    let packet = &mut *(*transfer.ptr).iso_packet_desc.as_mut_ptr().add(i);
                    packet.length = packet_size as u32;
                }
            })
    }
}

extern "system" fn transfer_callback(_transfer: *mut libusb_transfer) {}
