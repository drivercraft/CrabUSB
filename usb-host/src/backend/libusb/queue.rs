use libusb1_sys::{constants::*, *};
use log::*;
use usb_if::{
    err::TransferError,
    host::ResultTransfer,
    transfer::wait::{CallbackOnReady, WaitMap},
};

pub struct Queue {
    elems: Vec<USBTransfer>,
    wait_map: WaitMap<usize, Result<usize, TransferError>>,
    iter: usize,
}

impl Queue {
    pub fn new(size: usize) -> Self {
        let mut elems = Vec::with_capacity(size);
        for _ in 0..size {
            elems.push(USBTransfer::new(0));
        }
        let wait_map = WaitMap::new(0..size);
        Self {
            iter: 0,
            elems,
            wait_map,
        }
    }

    pub fn submit<'a, F>(&mut self, f: F) -> ResultTransfer<'a>
    where
        F: FnOnce(&mut USBTransfer, &mut CallbackOnReady),
    {
        let id = self.iter;
        self.wait_map.preper_id(&id)?;

        let user_data = Box::new(UserData {
            id,
            wait_map: &self.wait_map,
        });
        let user_data_ptr = Box::into_raw(user_data) as *mut std::ffi::c_void;

        let transfer = self.elems.get_mut(id).unwrap();
        let mut on_ready = CallbackOnReady {
            on_ready: |_, _, _| {},
            param1: transfer.ptr as *mut (),
            param2: std::ptr::null_mut(),
            param3: std::ptr::null_mut(),
        };
        unsafe {
            f(transfer, &mut on_ready);
            (*transfer.ptr).user_data = user_data_ptr;
            (*transfer.ptr).callback = transfer_callback;
        }
        usb!(libusb_submit_transfer(transfer.ptr))
            .map_err(|e| TransferError::Other(format!("Failed to submit transfer: {e:?}")))?;

        trace!("Submitted transfer id {id}");

        let res = self.wait_map.wait_for_result(id, Some(on_ready));
        self.iter = (self.iter + 1) % self.elems.len();
        Ok(res)
    }

    pub fn submit_iso<'a, F>(&mut self, num_iso_packets: usize, f: F) -> ResultTransfer<'a>
    where
        F: FnOnce(&mut USBTransfer, &mut CallbackOnReady),
    {
        let id = self.iter;
        self.wait_map.preper_id(&id)?;

        let user_data = Box::new(UserData {
            id,
            wait_map: &self.wait_map,
        });
        let user_data_ptr = Box::into_raw(user_data) as *mut std::ffi::c_void;

        // 为 ISO 传输创建新的传输结构
        let mut iso_transfer = USBTransfer::new(num_iso_packets);
        let mut on_ready = CallbackOnReady {
            on_ready: |_, _, _| {},
            param1: iso_transfer.ptr as *mut (),
            param2: std::ptr::null_mut(),
            param3: std::ptr::null_mut(),
        };

        unsafe {
            f(&mut iso_transfer, &mut on_ready);
            (*iso_transfer.ptr).user_data = user_data_ptr;
            (*iso_transfer.ptr).callback = transfer_callback;
        }

        let submit_result = usb!(libusb_submit_transfer(iso_transfer.ptr))
            .map_err(|e| TransferError::Other(format!("Failed to submit transfer: {e:?}")));

        if let Err(e) = submit_result {
            // 如果提交失败，清理分配的内存
            drop(iso_transfer);
            return Err(e);
        }

        trace!("Submitted ISO transfer id {id} with {num_iso_packets} packets");

        // 将 ISO 传输结构替换到队列中
        self.elems[id] = iso_transfer;

        let res = self.wait_map.wait_for_result(id, Some(on_ready));
        self.iter = (self.iter + 1) % self.elems.len();
        Ok(res)
    }
}

struct UserData<'a> {
    id: usize,
    wait_map: &'a WaitMap<usize, Result<usize, TransferError>>,
}

extern "system" fn transfer_callback(transfer: *mut libusb_transfer) {
    trace!("Transfer callback called");
    unsafe {
        let user_data_ptr = (*transfer).user_data as *mut UserData;
        if user_data_ptr.is_null() {
            return;
        }
        let user_data = Box::from_raw(user_data_ptr);
        let id = user_data.id;
        let wait_map = user_data.wait_map;

        let result = if (*transfer).status == LIBUSB_TRANSFER_COMPLETED {
            Ok((*transfer).actual_length as usize)
        } else {
            Err(TransferError::Other(format!(
                "Transfer failed with status: {:?}",
                match (*transfer).status {
                    LIBUSB_TRANSFER_COMPLETED => "COMPLETED",
                    LIBUSB_TRANSFER_ERROR => "ERROR",
                    LIBUSB_TRANSFER_TIMED_OUT => "TIMED_OUT",
                    LIBUSB_TRANSFER_CANCELLED => "CANCELLED",
                    LIBUSB_TRANSFER_STALL => "STALL",
                    LIBUSB_TRANSFER_NO_DEVICE => "NO_DEVICE",
                    LIBUSB_TRANSFER_OVERFLOW => "OVERFLOW",
                    _ => "UNKNOWN",
                }
            )))
        };
        wait_map.set_result(id, result);
    }
}

pub struct USBTransfer {
    pub ptr: *mut libusb_transfer,
    pub buff: Vec<u8>,
}

unsafe impl Send for USBTransfer {}

impl USBTransfer {
    pub fn new(iso_packets: usize) -> Self {
        let transfer = unsafe { libusb_alloc_transfer(iso_packets as i32) };
        Self {
            ptr: transfer,
            buff: vec![0u8; 512],
        }
    }
}

impl Drop for USBTransfer {
    fn drop(&mut self) {
        unsafe {
            libusb_free_transfer(self.ptr);
        }
    }
}
