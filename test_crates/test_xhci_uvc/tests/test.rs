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
use log::{info, warn};

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

            let mut format = formats.first().cloned().expect("no format found");

            for fmt in &formats {
                if fmt.frame_bytes() == 640 && fmt.height == 480 {
                    format = fmt.clone();
                    break;
                }
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
            let mut first_frame_captured = false;

            // 获取第一帧完整图像后跳出循环
            while !first_frame_captured {
                match stream.recv().await {
                    Ok(frames) => {
                        for frame in frames {
                            total_frames += 1;
                            info!(
                                "Received frame {}: {} bytes",
                                total_frames,
                                frame.data.len()
                            );

                            // 检查是否为完整帧（有EOF标志）
                            if frame.eof && !frame.data.is_empty() {
                                println!("=== CAPTURED FIRST COMPLETE FRAME ===");

                                // 输出视频格式信息到串口日志
                                println!("VIDEO_FORMAT_START");
                                println!("VIDEO_FORMAT: {:?}", current_format);
                                println!("VIDEO_FORMAT_END");

                                // 输出原始图像数据到串口日志（分块输出避免日志缓冲区溢出）
                                println!("FRAME_DATA_START");
                                println!("FRAME_SIZE: {}", frame.data.len());
                                println!("FRAME_NUMBER: {}", frame.frame_number);
                                if let Some(pts) = frame.pts_90khz {
                                    println!("FRAME_PTS: {}", pts);
                                }

                                // 将数据按4KB块分割输出，使用十六进制编码
                                const CHUNK_SIZE: usize = 4096;
                                let chunks = frame.data.chunks(CHUNK_SIZE);
                                let total_chunks = chunks.len();

                                for (chunk_idx, chunk) in chunks.enumerate() {
                                    // 将每个字节转换为十六进制字符串
                                    let hex_data = chunk
                                        .iter()
                                        .map(|b| alloc::format!("{:02x}", b))
                                        .collect::<Vec<_>>()
                                        .join("");

                                    println!(
                                        "CHUNK_{:04}_{:04}: {}",
                                        chunk_idx, total_chunks, hex_data
                                    );
                                }

                                println!("FRAME_DATA_END");

                                first_frame_captured = true;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        panic!("Error receiving frame: {:?}", e);
                    }
                }
            }

            info!("First frame captured successfully, stopping stream...");
            // 停止视频流
            drop(stream);
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
