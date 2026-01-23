#![cfg(target_os = "none")]
#![no_std]

extern crate alloc;

use core::{alloc::Layout, ptr::NonNull, time::Duration};

use bare_test::{
    mem::{PhysAddr, VirtAddr, alloc_with_mask, page_size},
    time::spin_delay,
};
use crab_usb::*;

pub struct KernelImpl;

impl Osal for KernelImpl {
    fn page_size(&self) -> usize {
        page_size()
    }

    unsafe fn map_single(
        &self,
        dma_mask: u64,
        addr: NonNull<u8>,
        size: usize,
        _direction: crate::Direction,
    ) -> Result<MapHandle, DmaError> {
        let mut phys = PhysAddr::from(VirtAddr::from(addr)).raw() as u64;
        if phys + size as u64 > dma_mask {
            let ptr = unsafe {
                alloc_with_mask(
                    Layout::from_size_align(size, self.page_size()).unwrap(),
                    dma_mask,
                )
            };
            if ptr.is_null() {
                return Err(DmaError::NoMemory);
            }
            phys = PhysAddr::from(VirtAddr::from(NonNull::new(ptr).unwrap())).raw() as u64;
        }
        Ok(MapHandle {
            dma_addr: phys,
            virt_addr: addr,
            size,
        })
    }

    unsafe fn unmap_single(&self, handle: MapHandle) {
        let vaddr = handle.virt_addr;
        let paddr = PhysAddr::from(VirtAddr::from(vaddr));
        let phys = paddr.raw() as u64;
        if phys + handle.size as u64 > handle.dma_addr {
            unsafe {
                alloc::alloc::dealloc(
                    vaddr.as_ptr(),
                    Layout::from_size_align(handle.size, self.page_size()).unwrap(),
                );
            }
        }
    }

    unsafe fn alloc_coherent(&self, dma_mask: u64, layout: Layout) -> Option<DmaHandle> {
        let ptr = unsafe { alloc_with_mask(layout, dma_mask) };
        if ptr.is_null() {
            None
        } else {
            Some(crab_usb::DmaHandle {
                virt_addr: NonNull::new(ptr).unwrap(),
                dma_addr: PhysAddr::from(VirtAddr::from(NonNull::new(ptr).unwrap())).raw() as _,
                layout,
            })
        }
    }

    unsafe fn dealloc_coherent(&self, _dma_mask: u64, handle: DmaHandle) {
        unsafe { alloc::alloc::dealloc(handle.virt_addr.as_ptr(), handle.layout) }
    }
}

impl KernelOp for KernelImpl {
    fn delay(&self, duration: Duration) {
        spin_delay(duration);
    }
}
