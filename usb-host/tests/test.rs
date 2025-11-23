#![no_std]
#![no_main]
#![feature(used_with_arg)]

extern crate alloc;

#[bare_test::tests]
mod tests {
    use alloc::{boxed::Box, vec::Vec};
    use bare_test::{
        GetIrqConfig,
        async_std::time,
        fdt_parser::{Fdt, Node, PciSpace, Status},
        globals::{PlatformInfoKind, global_val},
        irq::{IrqHandleResult, IrqInfo, IrqParam},
        mem::{iomap, page_size},
        platform::fdt::GetPciIrqConfig,
        println,
    };
    use core::time::Duration;
    use crab_usb::{impl_trait, *};
    use futures::FutureExt;
    use log::*;
    use pcie::*;
    use rockchip_pm::{RockchipPM, RkBoard, PD_USB, PD_PHP};
    use usb_if::{
        descriptor::{ConfigurationDescriptor, EndpointType},
        transfer::Direction,
    };

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

            for mut info in ls {
                info!("{info}");

                let mut interface_desc = None;
                let mut config_desc: Option<ConfigurationDescriptor> = None;
                for config in &info.configurations {
                    info!("config: {:?}", config.configuration_value);

                    for interface in &config.interfaces {
                        for alt in &interface.alt_settings {
                            info!(
                                "interface[{}.{}] class {:?}",
                                alt.interface_number,
                                alt.alternate_setting,
                                alt.class()
                            );
                            if interface_desc.is_none() {
                                interface_desc = Some(alt.clone());
                                config_desc = Some(config.clone());
                            }
                        }
                    }
                }
                let interface_desc = interface_desc.unwrap();
                let config_desc = config_desc.unwrap();

                let mut device = info.open().await.unwrap();

                info!("open device ok: {device:?}");

                device
                    .set_configuration(config_desc.configuration_value)
                    .await
                    .unwrap();
                info!("set configuration ok");

                // let config_value = device.current_configuration_descriptor().await.unwrap();
                // info!("get configuration: {config_value:?}");

                let mut interface = device
                    .claim_interface(
                        interface_desc.interface_number,
                        interface_desc.alternate_setting,
                    )
                    .await
                    .unwrap();
                info!(
                    "claim interface ok: {interface}  class {:?} subclass {:?}",
                    interface.descriptor.class, interface.descriptor.subclass
                );

                for ep_desc in &interface_desc.endpoints {
                    info!("endpoint: {ep_desc:?}");

                    match (ep_desc.transfer_type, ep_desc.direction) {
                        (EndpointType::Bulk, Direction::In) => {
                            let mut bulk_in = interface.endpoint_bulk_in(ep_desc.address).unwrap();
                            // You can use bulk_in to transfer data

                            let mut buff = alloc::vec![0u8; 64];
                            while let Ok(n) = bulk_in.submit(&mut buff).unwrap().await {
                                let data = &buff[..n];
                                info!("bulk in data: {data:?}",);
                                break; // For testing, break after first transfer
                            }
                        }
                        // (EndpointType::Isochronous, Direction::In) => {
                        //     let _iso_in = interface
                        //         .endpoint::<Isochronous, In>(ep_desc.address)
                        //         .unwrap();
                        //     // You can use iso_in to transfer data
                        // }
                        _ => {
                            info!(
                                "unsupported {:?} {:?}",
                                ep_desc.transfer_type, ep_desc.direction
                            );
                        }
                    }
                }

                // let mut _bulk_in = interface.endpoint::<Bulk, In>(0x81).unwrap();

                // let mut buff = alloc::vec![0u8; 64];

                // while let Ok(n) = bulk_in.transfer(&mut buff).await {
                //     let data = &buff[..n];

                //     info!("bulk in data: {data:?}",);
                // }

                drop(device);
            }
        });
    }

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

    struct XhciInfo {
        usb: USBHost,
        irq: Option<IrqInfo>,
    }

    fn get_usb_host_pcie() -> Option<XhciInfo> {
        let PlatformInfoKind::DeviceTree(fdt) = &global_val().platform_info;

        let fdt = fdt.get();

        let pcie = fdt
            .find_compatible(&["pci-host-ecam-generic", "brcm,bcm2711-pcie"])
            .next()?
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

                    return Some(XhciInfo {
                        usb: USBHost::new_xhci(addr, u32::MAX as usize),
                        irq,
                    });
                }
            }
        }
        None
    }

    fn get_usb_host() -> XhciInfo {
        if let Some(info) = get_usb_host_pcie() {
            return info;
        }

        let PlatformInfoKind::DeviceTree(fdt) = &global_val().platform_info;

        let fdt = fdt.get();
        let mut count = 0;
        for node in fdt.all_nodes() {
            if matches!(node.status(), Some(Status::Disabled)) {
                continue;
            }

            if node
                .compatibles()
                .any(|c| c.contains("xhci") | c.contains("snps,dwc3"))
            {
                println!("usb node: {}", node.name);
                let regs = node.reg().unwrap().collect::<Vec<_>>();
                println!("usb regs: {:?}", regs);

                ensure_rk3588_usb_power(&fdt, &node);

                let addr = iomap(
                    (regs[0].address as usize).into(),
                    regs[0].size.unwrap_or(0x1000),
                );

                let irq = node.irq_info();

                return XhciInfo {
                    usb: USBHost::new_xhci(addr, u32::MAX as usize),
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

    /// 打开 RK3588 USB 电源域（如果设备树有 power-domains 描述）
    fn ensure_rk3588_usb_power(fdt: &Fdt<'static>, usb_node: &Node<'static>) {
        let power_prop = match usb_node.find_property("power-domains") {
            Some(p) => p,
            None => {
                debug!("{} has no power-domains, skip PMU power on", usb_node.name());
                return;
            }
        };

        let mut ls = power_prop.u32_list();
        let _ctrl = match ls.next() {
            Some(v) => v,
            None => return,
        };
        let domain = match ls.next() {
            Some(v) => v,
            None => return,
        };

        debug!(
            "power-domains for {}: ctrl=0x{:x}, domain={}",
            usb_node.name(),
            _ctrl,
            domain
        );

        if domain != PD_USB.0 as u32 {
            debug!(
                "{} power domain is {}, not USB ({}) – skip",
                usb_node.name(),
                domain,
                PD_USB.0
            );
            return;
        }

        // 寻找 PMU syscon 基地址
        let pmu_node = fdt
            .all_nodes()
            .find(|n| n.compatibles().any(|c| c.contains("rk3588-pmu")))
            .or_else(|| fdt.all_nodes().find(|n| n.compatibles().any(|c| c.contains("rk3588-power-controller"))));

        let Some(pmu_node) = pmu_node else {
            warn!("rk3588 pmu node not found, skip powering usb domain");
            return;
        };

        let mut regs = match pmu_node.reg() {
            Some(r) => r,
            None => {
                warn!("pmu node without reg, skip powering usb domain");
                return;
            }
        };

        let Some(reg) = regs.next() else {
            warn!("pmu node reg empty, skip powering usb domain");
            return;
        };

        let start = (reg.address as usize) & !(page_size() - 1);
        let end = (reg.address as usize + reg.size.unwrap_or(0x1000) + page_size() - 1)
            & !(page_size() - 1);
        let base = iomap(start.into(), end - start);

        let mut pm = RockchipPM::new(base, RkBoard::Rk3588);
        for pd in [PD_USB, PD_PHP] {
            if let Err(e) = pm.power_domain_on(pd) {
                warn!("enable {:?} power domain failed: {e:?}", pd);
            } else {
                info!("enabled rk3588 {:?} power domain", pd);
            }
        }

        // 使能 VBUS 5V (vcc5v0_host) GPIO，如果存在的话
        if let Err(e) = enable_vcc5v0_host(fdt) {
            warn!("enable vcc5v0_host gpio failed: {:?}", e);
        }
    }

    #[derive(Debug)]
    enum GpioError {
        NotFound,
        RegMissing,
    }

    fn enable_vcc5v0_host(fdt: &Fdt<'static>) -> Result<(), GpioError> {
        // 在设备树中查找 regulator-name = "vcc5v0_host"
        let Some(reg_node) = fdt
            .all_nodes()
            .find(|n| n.find_property("regulator-name").map(|p| p.str()) == Some("vcc5v0_host"))
        else {
            return Err(GpioError::NotFound);
        };

        let gpio_prop = reg_node
            .find_property("gpio")
            .or_else(|| reg_node.find_property("gpios"))
            .ok_or(GpioError::NotFound)?;

        let mut vals = gpio_prop.u32_list();
        let ctrl = vals.next().ok_or(GpioError::NotFound)?;
        let pin = vals.next().ok_or(GpioError::NotFound)?;
        // flags ignored

        let ctrl_node = fdt
            .get_node_by_phandle(ctrl.into())
            .ok_or(GpioError::NotFound)?;

        let mut regs = ctrl_node.reg().ok_or(GpioError::RegMissing)?;
        let reg = regs.next().ok_or(GpioError::RegMissing)?;
        let base = iomap(
            (reg.address as usize).into(),
            reg.size.unwrap_or(0x1000).max(0x1000),
        );

        unsafe {
            // Rockchip GPIO: 0x00 DR, 0x04 DDR
            let dr = base.as_ptr() as *mut u32;
            let ddr = dr.add(1);

            let mut val = dr.read_volatile();
            val |= 1 << pin;
            dr.write_volatile(val);

            let mut dir = ddr.read_volatile();
            dir |= 1 << pin;
            ddr.write_volatile(dir);
        }

        info!(
            "vcc5v0_host enabled via gpio ctrl phandle 0x{:x}, pin {}",
            ctrl, pin
        );
        Ok(())
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
