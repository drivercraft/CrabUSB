#![no_std]
#![no_main]
#![feature(used_with_arg)]

extern crate alloc;
extern crate crab_usb;

use bare_test::{
    GetIrqConfig,
    async_std::time,
    fdt_parser::PciSpace,
    globals::{PlatformInfoKind, global_val},
    irq::{IrqHandleResult, IrqInfo, IrqParam},
    mem::page_size,
    platform::fdt::GetPciIrqConfig,
    println,
};
use core::time::Duration;
use crab_usb::{
    err::{TransferError, USBError},
    *,
};
use futures::FutureExt;
use log::{error, info, warn};

struct KernelImpl;
impl_trait! {
    impl Kernel for KernelImpl {
        fn sleep<'a>(duration: Duration) -> BoxFuture<'a, ()> {
            time::sleep(duration).boxed()
        }

        fn page_size() -> usize {
            page_size()
        }
    }
}

#[bare_test::tests]
mod tests {
    use alloc::{boxed::Box, vec::Vec};

    use bare_test::mem::iomap;
    use crab_uvc::{UvcDevice, VideoControlEvent};
    use log::*;
    use pcie::*;

    use super::*;

    #[test]
    fn test_all() {
        spin_on::spin_on(async {
            let info = get_usb_host();
            let irq_info = info.irq.clone().unwrap();

            let mut host = Box::pin(info.usb);

            register_irq(irq_info, &mut host);

            host.init().await.unwrap();
            info!("usb host init ok");
            info!("usb cmd test");

            let ls = host.device_list().await.unwrap();

            let mut dev_info = find_camera(ls).expect("no camera found");

            info!("found camera: {dev_info}");

            let dev = dev_info.open().await.unwrap();

            let mut uvc = UvcDevice::new(dev).await.unwrap();

            // 获取设备信息
            let device_info = uvc.get_device_info().await.unwrap();
            info!("Device info: {}", device_info);

            // 获取支持的视频格式
            let formats = uvc.get_supported_formats().await.unwrap();
            info!("Supported formats:");
            for format in &formats {
                info!("  {:?}", format);
            }

            // 设置视频格式 (选择第一个可用格式)
            if let Some(format) = formats.first() {
                info!("Setting format: {:?}", format);
                uvc.set_format(format.clone()).await.unwrap();
            } else {
                panic!("No supported formats available");
            }

            // 开始视频流
            info!("Starting video streaming...");
            let stream_result = uvc.start_streaming().await;
            let mut stream = match stream_result {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to start streaming: {:?}", e);
                    // 尝试降级到更低的带宽设置
                    info!("Retrying with different settings...");
                    panic!("Streaming failed: {:?}", e);
                }
            };

            // 获取当前视频格式信息
            let current_format = stream.vedio_format.clone();
            info!("Current video format: {:?}", current_format);

            // 设置一些控制参数的示例
            info!("Setting video controls...");

            // 尝试设置亮度（如果失败也继续）
            if let Err(e) = uvc
                .send_control_command(VideoControlEvent::BrightnessChanged(100))
                .await
            {
                warn!("Failed to set brightness: {:?}", e);
            }

            let mut total_frames = 0;
            let mut error_check_interval = 0;

            loop {
                match stream.recv().await {
                    Ok(frames) => {
                        for frame in frames {
                            total_frames += 1;
                            info!(
                                "Received frame {}: {} bytes",
                                total_frames,
                                frame.data.len()
                            );
                        }
                    }
                    Err(e) => {
                        // 检查是否是暂时性 XHCI 错误
                        if let USBError::TransferError(TransferError::Other(err_msg)) = &e {
                            if err_msg.contains("MissedServiceError")
                                || err_msg.contains("temporary")
                            {
                                warn!("Temporary XHCI error, retrying: {}", err_msg);
                                continue;
                            }
                        }
                        error!("Unrecoverable frame error: {:?}", e);
                        continue;
                    }
                }

                // 每处理100次接收后检查错误状态
                error_check_interval += 1;
                if error_check_interval % 100 == 0 {
                    let error_count = stream.error_packet_count();
                    if error_count > 0 {
                        warn!("Error packets detected: {}", error_count);

                        // 查询设备的流错误代码
                        match uvc.get_stream_error_code().await {
                            Ok(error_code) => {
                                if error_code != 0 {
                                    warn!("Device stream error code: 0x{:02x}", error_code);
                                }
                            }
                            Err(e) => {
                                warn!("Failed to get stream error code: {:?}", e);
                            }
                        }

                        // 重置计数器以避免重复警告
                        stream.reset_error_count();
                    }
                }
            }
        });
    }

    struct XhciInfo {
        usb: USBHost,
        irq: Option<IrqInfo>,
    }

    fn get_usb_host() -> XhciInfo {
        let PlatformInfoKind::DeviceTree(fdt) = &global_val().platform_info;

        let fdt = fdt.get();
        let pcie = fdt
            .find_compatible(&["pci-host-ecam-generic", "brcm,bcm2711-pcie"])
            .next()
            .unwrap()
            .into_pci()
            .unwrap();

        let mut pcie_regs = alloc::vec![];

        println!("pcie: {}", pcie.node.name);

        for reg in pcie.node.reg().unwrap() {
            println!(
                "pcie reg: {:#x}, bus: {:#x}",
                reg.address, reg.child_bus_address
            );
            let size = reg.size.unwrap_or_default().align_up(0x1000);

            pcie_regs.push(iomap((reg.address as usize).into(), size));
        }

        let mut bar_alloc = SimpleBarAllocator::default();

        for range in pcie.ranges().unwrap() {
            info!("pcie range: {range:?}");

            match range.space {
                PciSpace::Memory32 => bar_alloc.set_mem32(range.cpu_address as _, range.size as _),
                PciSpace::Memory64 => bar_alloc.set_mem64(range.cpu_address, range.size),
                _ => {}
            }
        }

        let base_vaddr = pcie_regs[0];

        info!("Init PCIE @{base_vaddr:?}");

        let mut root = RootComplexGeneric::new(base_vaddr);

        // for elem in root.enumerate_keep_bar(None) {
        for elem in root.enumerate(None, Some(bar_alloc)) {
            debug!("PCI {elem}");

            if let Header::Endpoint(mut ep) = elem.header {
                ep.update_command(elem.root, |mut cmd| {
                    cmd.remove(CommandRegister::INTERRUPT_DISABLE);
                    cmd | CommandRegister::IO_ENABLE
                        | CommandRegister::MEMORY_ENABLE
                        | CommandRegister::BUS_MASTER_ENABLE
                });

                for cap in &mut ep.capabilities {
                    match cap {
                        PciCapability::Msi(msi_capability) => {
                            msi_capability.set_enabled(false, &mut *elem.root);
                        }
                        PciCapability::MsiX(msix_capability) => {
                            msix_capability.set_enabled(false, &mut *elem.root);
                        }
                        _ => {}
                    }
                }

                println!("irq_pin {:?}, {:?}", ep.interrupt_pin, ep.interrupt_line);

                if matches!(ep.device_type(), DeviceType::UsbController) {
                    let bar_addr;
                    let mut bar_size;
                    match ep.bar {
                        pcie::BarVec::Memory32(bar_vec_t) => {
                            let bar0 = bar_vec_t[0].as_ref().unwrap();
                            bar_addr = bar0.address as usize;
                            bar_size = bar0.size as usize;
                        }
                        pcie::BarVec::Memory64(bar_vec_t) => {
                            let bar0 = bar_vec_t[0].as_ref().unwrap();
                            bar_addr = bar0.address as usize;
                            bar_size = bar0.size as usize;
                        }
                        pcie::BarVec::Io(_bar_vec_t) => todo!(),
                    };

                    println!("bar0: {:#x}", bar_addr);
                    println!("bar0 size: {:#x}", bar_size);
                    bar_size = bar_size.align_up(0x1000);
                    println!("bar0 size algin: {:#x}", bar_size);

                    let addr = iomap(bar_addr.into(), bar_size);
                    trace!("pin {:?}", ep.interrupt_pin);

                    let irq = pcie.child_irq_info(
                        ep.address.bus(),
                        ep.address.device(),
                        ep.address.function(),
                        ep.interrupt_pin,
                    );

                    println!("irq: {irq:?}");

                    return XhciInfo {
                        usb: USBHost::new_xhci(addr),
                        irq,
                    };
                }
            }
        }

        for node in fdt.all_nodes() {
            if node.compatibles().any(|c| c.contains("xhci")) {
                println!("usb node: {}", node.name);
                let regs = node.reg().unwrap().collect::<Vec<_>>();
                println!("usb regs: {:?}", regs);

                let addr = iomap(
                    (regs[0].address as usize).into(),
                    regs[0].size.unwrap_or(0x1000),
                );

                let irq = node.irq_info();

                return XhciInfo {
                    usb: USBHost::new_xhci(addr),
                    irq,
                };
            }
        }

        panic!("no xhci found");
    }

    fn register_irq(irq: IrqInfo, host: &mut USBHost) {
        let handle = host.event_handler();

        for one in &irq.cfgs {
            IrqParam {
                intc: irq.irq_parent,
                cfg: one.clone(),
            }
            .register_builder({
                move |_irq| {
                    handle.handle_event();
                    IrqHandleResult::Handled
                }
            })
            .register();
            break;
        }
    }

    fn find_camera(ls: impl Iterator<Item = DeviceInfo>) -> Option<DeviceInfo> {
        for info in ls {
            if UvcDevice::check(&info) {
                return Some(info);
            }
        }
        None
    }
}

trait Align {
    fn align_up(&self, align: usize) -> usize;
}

impl Align for usize {
    fn align_up(&self, align: usize) -> usize {
        if (*self).is_multiple_of(align) {
            *self
        } else {
            *self + align - *self % align
        }
    }
}
