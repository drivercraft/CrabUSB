#![cfg(target_os = "none")]
#![no_std]

extern crate alloc;

use core::{alloc::Layout, num::NonZeroUsize, ptr::NonNull, time::Duration};

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
        size: NonZeroUsize,
        _direction: crate::Direction,
    ) -> Result<MapHandle, DmaError> {
        let size = size.get();
        let orig_phys = PhysAddr::from(VirtAddr::from(addr)).raw() as u64;

        if orig_phys + size as u64 > dma_mask {
            // 需要重新分配内存
            let ptr = unsafe {
                alloc_with_mask(
                    Layout::from_size_align(size, self.page_size()).unwrap(),
                    dma_mask,
                )
            };
            if ptr.is_null() {
                return Err(DmaError::NoMemory);
            }

            let new_virt = NonNull::new(ptr).unwrap();
            let new_phys = PhysAddr::from(VirtAddr::from(new_virt)).raw() as u64;

            log::debug!(
                "DMA remap: orig_virt={:#x}, orig_phys={:#x} -> new_virt={:#x}, new_phys={:#x}, size={:#x}",
                addr.as_ptr() as usize,
                orig_phys,
                new_virt.as_ptr() as usize,
                new_phys,
                size
            );

            // ✅ 返回新分配的虚拟地址和物理地址
            Ok(MapHandle {
                dma_addr: new_phys,
                virt_addr: new_virt,
                size,
            })
        } else {
            // ✅ 原始地址可以使用，直接返回
            Ok(MapHandle {
                dma_addr: orig_phys,
                virt_addr: addr,
                size,
            })
        }
    }

    unsafe fn unmap_single(&self, handle: MapHandle) {
        let vaddr = handle.virt_addr;
        let virt_addr_as_phys = PhysAddr::from(VirtAddr::from(vaddr)).raw() as u64;

        // ✅ 核心逻辑：通过对比虚拟地址对应的物理地址和 handle 中的 dma_addr
        // 来判断是否重新分配了内存
        //
        // 如果 virt_addr 对应的物理地址 != handle.dma_addr
        // 说明我们在 map_single 中重新分配了内存
        if virt_addr_as_phys != handle.dma_addr {
            log::debug!(
                "DMA unmap: freeing reallocated memory: virt={:#x}, virt_phys={:#x}, dma_addr={:#x}, size={:#x}",
                vaddr.as_ptr() as usize,
                virt_addr_as_phys,
                handle.dma_addr,
                handle.size
            );
            // 重新分配过，需要释放新分配的内存
            unsafe {
                alloc::alloc::dealloc(
                    vaddr.as_ptr(),
                    Layout::from_size_align(handle.size, self.page_size()).unwrap(),
                );
            }
        } else {
            log::debug!(
                "DMA unmap: skipping original buffer: virt={:#x}, phys={:#x}, size={:#x}",
                vaddr.as_ptr() as usize,
                virt_addr_as_phys,
                handle.size
            );
        }
        // 如果 virt_addr_as_phys == handle.dma_addr
        // 说明没有重新分配，原始 buffer 由调用者管理，不需要释放
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
