use core::ptr::null_mut;
use std::{
    collections::HashMap,
    sync::{Arc, Weak, atomic::AtomicBool},
};

use futures::task::AtomicWaker;
use libusb1_sys::{
    libusb_device_handle, libusb_fill_bulk_transfer, libusb_fill_control_setup,
    libusb_fill_control_transfer, libusb_fill_interrupt_transfer, libusb_fill_iso_transfer,
    libusb_get_iso_packet_buffer_simple, libusb_transfer,
};
use log::trace;
use usb_if::{err::TransferError, host::USBError, transfer::BmRequestType};

use crate::{
    EndpointOp,
    backend::{
        libusb::{device::DeviceHandle, err::transfer_status_to_result},
        ty::transfer::{Transfer, TransferKind},
    },
};

pub struct EndpointImpl {
    dev: Arc<DeviceHandle>,
    address: u8,
    transfers: HashMap<u64, Arc<TransferHandleRaw>>,
}

impl EndpointImpl {
    pub fn new(dev: Arc<DeviceHandle>, address: u8) -> Self {
        Self {
            dev,
            address,
            transfers: HashMap::new(),
        }
    }

    fn make_transfer(
        &mut self,
        transfer: Transfer,
    ) -> Result<Arc<TransferHandleRaw>, TransferError> {
        let trans_ptr = unsafe { libusb1_sys::libusb_alloc_transfer(0) };
        if trans_ptr.is_null() {
            return Err(TransferError::Other("no memory".into()));
        }
        let trans_handle = Arc::new(TransferHandleRaw {
            transfer: trans_ptr,
            waker: AtomicWaker::new(),
            ok: AtomicBool::new(false),
        });

        let dev_handle = self.dev.raw();
        let buffer = transfer.buffer_addr as *mut u8;
        let length = transfer.buffer_len as i32;
        let timeout = 1000; // TODO: make it configurable
        let weak = Arc::downgrade(&trans_handle);
        let user_data = Weak::into_raw(weak) as *mut core::ffi::c_void;

        match transfer.kind {
            TransferKind::Control(setup) => {
                unsafe {
                    libusb_fill_control_setup(
                        buffer,
                        BmRequestType::new(transfer.direction, setup.request_type, setup.recipient)
                            .into(),
                        setup.request.into(),
                        setup.value,
                        setup.index,
                        length as _,
                    );
                    libusb_fill_control_transfer(
                        trans_ptr,
                        dev_handle,
                        buffer,
                        transfer_callback,
                        user_data,
                        timeout,
                    )
                };
            }
            TransferKind::Bulk => {
                unsafe {
                    libusb_fill_bulk_transfer(
                        trans_ptr,
                        dev_handle,
                        self.address,
                        buffer,
                        length,
                        transfer_callback,
                        user_data,
                        timeout,
                    )
                };
            }
            TransferKind::Interrupt => {
                unsafe {
                    libusb_fill_bulk_transfer(
                        trans_ptr,
                        dev_handle,
                        self.address,
                        buffer,
                        length,
                        transfer_callback,
                        user_data,
                        timeout,
                    )
                };
            }
            TransferKind::Isochronous { num_pkgs } => {
                unsafe {
                    libusb_fill_iso_transfer(
                        trans_ptr,
                        dev_handle,
                        self.address,
                        buffer,
                        length,
                        num_pkgs as _,
                        transfer_callback,
                        user_data,
                        timeout,
                    )
                };

                // 设置每个 ISO packet 的长度，防止溢出
                let packet_size = length / num_pkgs as i32;
                for i in 0..num_pkgs {
                    let packet = unsafe { &mut *(*trans_ptr).iso_packet_desc.as_mut_ptr().add(i) };
                    packet.length = packet_size as u32;
                }
            }
        }

        Ok(trans_handle)
    }
}

unsafe impl Send for EndpointImpl {}

impl EndpointOp for EndpointImpl {
    fn submit(
        &mut self,
        transfer: crate::backend::ty::transfer::Transfer,
    ) -> Result<crate::TransferHandle<'_>, usb_if::err::TransferError> {
        let trans = self.make_transfer(transfer)?;
        let id = trans.id();
        self.transfers.insert(id, trans);

        Ok(crate::TransferHandle::new(id, self))
    }

    fn query_transfer(
        &mut self,
        id: u64,
    ) -> Option<Result<crate::backend::ty::transfer::Transfer, usb_if::err::TransferError>> {
        let trans = self.transfers.get(&id)?;
        if !trans.ok.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        let trans = self.transfers.remove(&id).unwrap();
        Some(trans.to_result())
    }

    fn register_cx(&self, id: u64, cx: &mut std::task::Context<'_>) {
        if let Some(trans) = self.transfers.get(&id) {
            trans.register_waker(cx);
        }
    }
}

struct TransferHandleRaw {
    transfer: *mut libusb_transfer,
    ok: AtomicBool,
    waker: AtomicWaker,
}

unsafe impl Send for TransferHandleRaw {}
unsafe impl Sync for TransferHandleRaw {}

impl TransferHandleRaw {
    fn register_waker(&self, cx: &mut std::task::Context<'_>) {
        self.waker.register(cx.waker());
    }

    fn to_result(
        &self,
    ) -> Result<crate::backend::ty::transfer::Transfer, usb_if::err::TransferError> {
        transfer_status_to_result(unsafe { (*self.transfer).status })?;
        let trans_raw = unsafe { &*self.transfer };
        let trans = crate::backend::ty::transfer::Transfer {
            kind: todo!(),
            direction: todo!(),
            buffer_addr: todo!(),
            buffer_len: todo!(),
            transfer_len: trans_raw.actual_length as usize,
        };

        Ok(trans)
    }

    fn id(&self) -> u64 {
        self.transfer as usize as u64
    }
}

impl Drop for TransferHandleRaw {
    fn drop(&mut self) {
        unsafe {
            libusb1_sys::libusb_free_transfer(self.transfer);
        }
    }
}

extern "system" fn transfer_callback(transfer: *mut libusb_transfer) {
    let user_data = unsafe { (*transfer).user_data };
    if user_data.is_null() {
        return;
    }
    let weak: Weak<TransferHandleRaw> =
        unsafe { Weak::from_raw(user_data as *const TransferHandleRaw) };

    if let Some(trans_handle) = weak.upgrade() {
        trace!("libusb transfer callback called, transfer={:p}", transfer);
        trans_handle
            .ok
            .store(true, std::sync::atomic::Ordering::Release);
        trans_handle.waker.wake();
    }
}
