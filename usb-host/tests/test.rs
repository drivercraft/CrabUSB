#![no_std]
#![no_main]
#![feature(used_with_arg)]
#![allow(dead_code)]

extern crate alloc;

#[bare_test::tests]
mod tests {
    use alloc::{boxed::Box, vec::Vec};
    use bare_test::{
        GetIrqConfig,
        async_std::time::{self, sleep},
        fdt_parser::{Fdt, Node, PciSpace, Status},
        globals::{PlatformInfoKind, global_val},
        irq::{IrqHandleResult, IrqInfo, IrqParam},
        mem::{iomap, page_size},
        platform::fdt::GetPciIrqConfig,
        println,
    };
    use core::{
        hint::spin_loop,
        sync::atomic::{AtomicBool, Ordering},
        time::Duration,
    };
    use crab_usb::{impl_trait, *};
    use futures::{FutureExt, future::BoxFuture};
    use log::info;
    use log::*;
    use pcie::*;
    use rockchip_pm::{PowerDomain, RockchipPM};
    use usb_if::{
        descriptor::{ConfigurationDescriptor, EndpointType},
        transfer::Direction,
    };

    use super::*;

    static PROT_CHANGED: AtomicBool = AtomicBool::new(false);

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

            let mut timeout = 100;
            while !PROT_CHANGED.load(Ordering::Acquire) {
                sleep(Duration::from_millis(100)).await;
                timeout -= 1;
                if timeout == 0 {
                    panic!("timeout waiting for port change");
                }
            }
            info!("port changed detected");
            let ls = host.probe_devices().await.unwrap();

            for mut info in ls {
                info!("{info:#x?}");

                let mut interface_desc = None;
                let mut config_desc: Option<ConfigurationDescriptor> = None;
                for config in info.configurations() {
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
                //     let interface_desc = interface_desc.unwrap();
                //     let config_desc = config_desc.unwrap();

                //     let mut device = info.open().await.unwrap();

                //     info!("open device ok: {device:?}");

                //     device
                //         .set_configuration(config_desc.configuration_value)
                //         .await
                //         .unwrap();
                //     info!("set configuration ok");

                //     // let config_value = device.current_configuration_descriptor().await.unwrap();
                //     // info!("get configuration: {config_value:?}");

                //     let mut interface = device
                //         .claim_interface(
                //             interface_desc.interface_number,
                //             interface_desc.alternate_setting,
                //         )
                //         .await
                //         .unwrap();
                //     info!(
                //         "claim interface ok: {interface}  class {:?} subclass {:?}",
                //         interface.descriptor.class, interface.descriptor.subclass
                //     );

                //     for ep_desc in &interface_desc.endpoints {
                //         info!("endpoint: {ep_desc:?}");

                //         match (ep_desc.transfer_type, ep_desc.direction) {
                //             (EndpointType::Bulk, Direction::In) => {
                //                 let mut bulk_in = interface.endpoint_bulk_in(ep_desc.address).unwrap();
                //                 // You can use bulk_in to transfer data

                //                 let mut buff = alloc::vec![0u8; 64];
                //                 while let Ok(n) = bulk_in.submit(&mut buff).unwrap().await {
                //                     let data = &buff[..n];
                //                     info!("bulk in data: {data:?}",);
                //                     break; // For testing, break after first transfer
                //                 }
                //             }
                //             // (EndpointType::Isochronous, Direction::In) => {
                //             //     let _iso_in = interface
                //             //         .endpoint::<Isochronous, In>(ep_desc.address)
                //             //         .unwrap();
                //             //     // You can use iso_in to transfer data
                //             // }
                //             _ => {
                //                 info!(
                //                     "unsupported {:?} {:?}",
                //                     ep_desc.transfer_type, ep_desc.direction
                //                 );
                //             }
                //         }
                //     }

                //     // let mut _bulk_in = interface.endpoint::<Bulk, In>(0x81).unwrap();

                //     // let mut buff = alloc::vec![0u8; 64];

                //     // while let Ok(n) = bulk_in.transfer(&mut buff).await {
                //     //     let data = &buff[..n];

                //     //     info!("bulk in data: {data:?}",);
                // }

                // drop(device);
            }
        });
    }

    struct KernelImpl;
    impl_trait! {
        impl Kernel for KernelImpl {
            fn page_size() -> usize {
                page_size()
            }
        }
    }

    struct XhciInfo {
        usb: USBHost<Xhci>,
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
                        usb: USBHost::new_xhci(addr, u32::MAX as usize).unwrap(),
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
        for node in fdt.all_nodes() {
            if matches!(node.status(), Some(Status::Disabled)) {
                continue;
            }

            if node
                .compatibles()
                .any(|c| c.contains("xhci") | c.contains("snps,dwc3"))
            {
                // 只选择明确为 host 模式的控制器，避免误用 OTG 端口
                if let Some(prop) = node.find_property("dr_mode") {
                    let mode = prop.str();
                    if mode != "host" {
                        debug!("skip {} because dr_mode={}", node.name(), mode);
                        continue;
                    }
                }

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
                    usb: USBHost::new_xhci(addr, u32::MAX as usize).unwrap(),
                    irq,
                };
            }
        }

        panic!("no xhci found");
    }

    fn register_irq(irq: IrqInfo, host: &mut USBHost<Xhci>) {
        let handle = host.create_event_handler();

        for one in &irq.cfgs {
            IrqParam {
                intc: irq.irq_parent,
                cfg: one.clone(),
            }
            .register_builder({
                move |_irq| {
                    let event = handle.handle_event();
                    match event {
                        Event::PortChange { .. } => {
                            PROT_CHANGED.store(true, Ordering::Release);
                        }
                        _ => {}
                    }

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
                debug!(
                    "{} has no power-domains, skip PMU power on",
                    usb_node.name()
                );
                return;
            }
        };

        let mut ls = power_prop.u32_list();
        let ctrl_phandle = match ls.next() {
            Some(v) => v,
            None => return,
        };
        let domains: Vec<u32> = ls.collect();
        if domains.is_empty() {
            debug!(
                "{} power-domains has no domain IDs, skip PMU power on",
                usb_node.name()
            );
            return;
        }

        debug!(
            "power-domains for {}: ctrl=0x{:x}, domains={:?}",
            usb_node.name(),
            ctrl_phandle,
            domains
        );

        // 精确查找 PMU syscon 基址：compatible 必须等于 "rockchip,rk3588-pmu"
        let pmu_node = fdt
            .all_nodes()
            .find(|n| n.compatibles().any(|c| c == "rockchip,rk3588-pmu"))
            .or_else(|| {
                // 兜底：按照 power-domains ctrl phandle 找 power-controller 本身
                fdt.get_node_by_phandle(ctrl_phandle.into())
            });

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

        let compatible = pmu_node
            .compatibles()
            .find(|c| {
                matches!(
                    *c,
                    "rockchip,rk3588-power-controller" | "rockchip,rk3568-power-controller"
                )
            })
            .unwrap_or("rockchip,rk3588-power-controller");

        let mut pm = RockchipPM::new_with_compatible(base, compatible);

        // Power on domains described by DT, then ensure PHP (bus fabric) is on.
        let mut pd_list: Vec<PowerDomain> =
            domains.iter().map(|id| PowerDomain::from(*id)).collect();

        if let Some(php_pd) = pm.get_power_dowain_by_name("php") {
            // Avoid duplicates if DT already lists PHP.
            if !pd_list.contains(&php_pd) {
                pd_list.push(php_pd);
            }
        }

        for pd in pd_list {
            if let Err(e) = pm.power_domain_on(pd) {
                warn!("enable {:?} power domain failed: {e:?}", pd);
            } else {
                info!("enabled rk3588 power domain {:?}", pd);
            }
        }

        info!("=== Initial PHY state BEFORE any configuration ===");
        dump_initial_phy_state();

        info!("=== Attempting device power cycle via VBUS toggle ===");
        info!("GL3523 hub research suggests: 1s VBUS off, then 300ms before controller init");
        if let Err(e) = disable_vcc5v0_host(fdt) {
            warn!("disable vcc5v0_host gpio failed: {:?}", e);
        }
        info!("Waiting 1000ms with VBUS disabled (full hub reset)...");
        spin_delay_ms(1000);
        info!("Re-enabling VBUS BEFORE any init (hub needs power to respond)...");
        if let Err(e) = enable_vcc5v0_host(fdt) {
            warn!("enable vcc5v0_host gpio failed: {:?}", e);
        }
        info!("Waiting 300ms for hub internal initialization...");
        spin_delay_ms(300);

        info!("=== TRY #1: USB3 mode with full init ===");
        let usbgrf_base = iomap(0xfd5ac000.into(), 0x1000);
        unsafe {
            let ptr = usbgrf_base.as_ptr().add(0x0034) as *mut u32;
            let val: u32 = (0xFFFF << 16) | 0x1100;
            ptr.write_volatile(val);
            let con1 = (usbgrf_base.as_ptr().add(0x0034) as *const u32).read_volatile();
            info!(
                "USB_GRF USB3OTG1_CON1 set to {:#010x} (USB3 mode, host_num_u3_port=1)",
                con1
            );
        }

        if let Err(e) = init_usb2phy1_full(fdt) {
            warn!("init usb2phy1 full failed: {:?}", e);
        }

        if let Err(e) = init_usbdp_phy1_full(fdt) {
            warn!("init usbdp phy1 full failed: {:?}", e);
        }

        if let Err(e) = deassert_rk3588_usb_resets(fdt) {
            warn!("deassert usb resets failed: {:?}", e);
        }

        set_dwc3_host_mode_early();

        init_dwc3_uboot_style();
        info!("Waiting 50ms after DWC3 soft reset...");
        spin_delay_ms(50);
        info!("Re-waiting for PHY PLL lock after DWC3 soft reset...");
        let pma_base = iomap(0xfed98000.into(), 0x10000);
        unsafe {
            for i in 0..100 {
                let lcpll = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
                if (lcpll & 0xC0) == 0xC0 {
                    info!("LCPLL re-locked after {} iterations: {:#04x}", i, lcpll);
                    break;
                }
                if i == 99 {
                    warn!("LCPLL failed to re-lock! val={:#04x}", lcpll);
                }
                spin_delay_ms(5);
            }
        }
        info!("Waiting 300ms for PHY to stabilize...");
        spin_delay_ms(300);
        if !check_phy_lock_status_and_recover() {
            warn!("PHY failed to lock");
        }
        check_port_status("after DWC3 config");
        info!("Starting xHCI controller...");
        start_xhci_controller();
        info!("Waiting 500ms for link training (checking PLL every 100ms)...");
        let pma_base_check = iomap(0xfed98000.into(), 0x10000);
        for i in 0..5 {
            spin_delay_ms(100);
            let lcpll =
                unsafe { (pma_base_check.as_ptr().add(0x0350) as *const u32).read_volatile() };
            let locked = (lcpll & 0xC0) == 0xC0;
            info!(
                "  {}ms: LCPLL={:#04x} locked={}",
                (i + 1) * 100,
                lcpll,
                locked
            );
            if !locked {
                info!("  PHY PLL lost lock during wait!");
            }
        }
        check_port_status("after xHCI start + 500ms wait");
        // Skip StartRxDetU3RxDet - it causes PHY PLL to lose lock
        // info!("=== Trying manual receiver detection via StartRxDetU3RxDet ===");
        // try_force_rx_detect();
        // spin_delay_ms(200);
        // check_port_status("after StartRxDetU3RxDet trigger");
        // Try longer wait before warm reset
        info!("Waiting 2 seconds for device to potentially connect...");
        spin_delay_ms(2000);
        check_port_status("after 2s wait");
        try_warm_port_reset();
        info!("Waiting 200ms more after warm reset...");
        spin_delay_ms(200);
        check_port_status("after warm reset");
        let grf_base2 = iomap(0xfd5d4000.into(), 0x8000);
        unsafe {
            let read_grf =
                |off: usize| -> u32 { (grf_base2.as_ptr().add(off) as *const u32).read_volatile() };
            let phy_status0 = read_grf(0x4000 + 0xC0);
            let linestate = (phy_status0 >> 9) & 0x3;
            info!(
                "USB2PHY1 STATUS0 after wait: {:#010x} (linestate={:02b})",
                phy_status0, linestate
            );
        }

        info!("=== TRY #2: Port Power Cycle (PP bit toggle) ===");
        try_port_power_cycle();
        info!("=== TRY #3: USB2-ONLY mode (disable SuperSpeed) ===");
        let usbgrf_base2 = iomap(0xfd5ac000.into(), 0x1000);
        unsafe {
            let ptr = usbgrf_base2.as_ptr().add(0x0034) as *mut u32;
            let val: u32 = (0xFFFF << 16) | 0x0188;
            ptr.write_volatile(val);
            let con1 = (usbgrf_base2.as_ptr().add(0x0034) as *const u32).read_volatile();
            info!(
                "USB_GRF USB3OTG1_CON1 set to {:#010x} (USB2-only mode, host_num_u3_port=0)",
                con1
            );
        }
        info!("Waiting 500ms for USB2 detection...");
        spin_delay_ms(500);
        check_port_status("after USB2-only mode switch");
        unsafe {
            let read_grf =
                |off: usize| -> u32 { (grf_base2.as_ptr().add(off) as *const u32).read_volatile() };
            let phy_status0 = read_grf(0x4000 + 0xC0);
            let linestate = (phy_status0 >> 9) & 0x3;
            info!(
                "USB2PHY1 STATUS0 (USB2 mode): {:#010x} (linestate={:02b})",
                phy_status0, linestate
            );
        }

        info!("=== Restore USB3 mode ===");
        unsafe {
            let ptr = usbgrf_base2.as_ptr().add(0x0034) as *mut u32;
            let val: u32 = (0xFFFF << 16) | 0x1100;
            ptr.write_volatile(val);
            let con1 = (usbgrf_base2.as_ptr().add(0x0034) as *const u32).read_volatile();
            info!(
                "USB_GRF USB3OTG1_CON1 restored to {:#010x} (USB3 mode)",
                con1
            );
        }

        info!("=== TRY #4: VBUS GPIO Toggle (hardware power cycle) ===");
        try_vbus_gpio_toggle();
        check_port_status("after VBUS GPIO toggle");

        dump_usb_debug_registers();
    }

    fn try_force_rx_detect() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };
        unsafe {
            let read_pipe =
                || -> u32 { (dwc3_base.add(GUSB3PIPECTL) as *const u32).read_volatile() };
            let write_pipe = |val: u32| {
                (dwc3_base.add(GUSB3PIPECTL) as *mut u32).write_volatile(val);
            };

            let pipe_before = read_pipe();
            info!("GUSB3PIPECTL before RxDet: {:#010x}", pipe_before);

            let mut pipe = pipe_before;
            pipe |= GUSB3PIPECTL_STARTRXDETU3RXDET;
            write_pipe(pipe);
            info!("Set StartRxDetU3RxDet=1 (pulse)");

            spin_delay_ms(10);

            pipe = read_pipe();
            pipe &= !GUSB3PIPECTL_STARTRXDETU3RXDET;
            write_pipe(pipe);
            info!("Cleared StartRxDetU3RxDet");

            let pipe_after = read_pipe();
            info!("GUSB3PIPECTL after RxDet: {:#010x}", pipe_after);
        }
    }

    fn try_port_power_cycle() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        unsafe {
            let caplength = (xhci_base.as_ptr().add(0x00) as *const u8).read_volatile();
            let op_base = xhci_base.as_ptr().add(caplength as usize);

            let portsc_usb2_ptr = op_base.add(0x400) as *mut u32;
            let portsc_usb3_ptr = op_base.add(0x410) as *mut u32;

            let portsc_usb2 = portsc_usb2_ptr.read_volatile();
            let portsc_usb3 = portsc_usb3_ptr.read_volatile();
            info!(
                "Before PP cycle: USB2 PORTSC={:#010x}, USB3 PORTSC={:#010x}",
                portsc_usb2, portsc_usb3
            );

            let portsc_usb2_pp_off = (portsc_usb2 & PORTSC_RW_MASK) & !PORTSC_PP_BIT;
            let portsc_usb3_pp_off = (portsc_usb3 & PORTSC_RW_MASK) & !PORTSC_PP_BIT;

            info!("Powering OFF ports (clearing PP bit)...");
            portsc_usb2_ptr.write_volatile(portsc_usb2_pp_off);
            portsc_usb3_ptr.write_volatile(portsc_usb3_pp_off);

            info!("Waiting 500ms with port power OFF...");
            spin_delay_ms(500);

            let portsc_usb2_off = portsc_usb2_ptr.read_volatile();
            let portsc_usb3_off = portsc_usb3_ptr.read_volatile();
            info!(
                "After PP off: USB2 PORTSC={:#010x} (PP={}), USB3 PORTSC={:#010x} (PP={})",
                portsc_usb2_off,
                (portsc_usb2_off >> 9) & 1,
                portsc_usb3_off,
                (portsc_usb3_off >> 9) & 1
            );

            let portsc_usb2_pp_on = (portsc_usb2_off & PORTSC_RW_MASK) | PORTSC_PP_BIT;
            let portsc_usb3_pp_on = (portsc_usb3_off & PORTSC_RW_MASK) | PORTSC_PP_BIT;

            info!("Powering ON ports (setting PP bit)...");
            portsc_usb2_ptr.write_volatile(portsc_usb2_pp_on);
            portsc_usb3_ptr.write_volatile(portsc_usb3_pp_on);

            info!("Waiting 500ms for device detection after PP on...");
            spin_delay_ms(500);

            let portsc_usb2_on = portsc_usb2_ptr.read_volatile();
            let portsc_usb3_on = portsc_usb3_ptr.read_volatile();
            let ccs_usb2 = portsc_usb2_on & 1;
            let ccs_usb3 = portsc_usb3_on & 1;
            let pls_usb2 = (portsc_usb2_on >> 5) & 0xf;
            let pls_usb3 = (portsc_usb3_on >> 5) & 0xf;

            info!(
                "After PP on: USB2 PORTSC={:#010x} (CCS={}, PLS={}), USB3 PORTSC={:#010x} (CCS={}, PLS={})",
                portsc_usb2_on, ccs_usb2, pls_usb2, portsc_usb3_on, ccs_usb3, pls_usb3
            );

            if ccs_usb2 == 1 || ccs_usb3 == 1 {
                info!("SUCCESS: Device detected after port power cycle!");
            } else {
                info!("Device still not detected after port power cycle");
            }
        }
    }

    /// Toggle VBUS power via GPIO3_B7 (vcc5v0_host regulator enable)
    /// This is a more aggressive reset that cuts power to the GL3523 hub
    fn try_vbus_gpio_toggle() {
        let gpio3_base = iomap(GPIO3_BASE.into(), 0x1000);
        unsafe {
            // Read current state
            let dr_before =
                (gpio3_base.as_ptr().add(GPIO_SWPORT_DR_L) as *const u32).read_volatile();
            let ddr_before =
                (gpio3_base.as_ptr().add(GPIO_SWPORT_DDR_L) as *const u32).read_volatile();
            info!(
                "GPIO3 before: DR_L={:#010x}, DDR_L={:#010x}",
                dr_before, ddr_before
            );
            info!(
                "  GPIO3_B7 (bit15): value={}, direction={} (1=output)",
                (dr_before >> 15) & 1,
                (ddr_before >> 15) & 1
            );

            // Ensure GPIO3_B7 is configured as output
            let ddr_ptr = gpio3_base.as_ptr().add(GPIO_SWPORT_DDR_L) as *mut u32;
            ddr_ptr.write_volatile(WRITE_MASK_BIT15 | GPIO3_B7_BIT);

            // Turn OFF VBUS (set GPIO3_B7 low - active high regulator)
            info!("Turning OFF VBUS (GPIO3_B7 = 0)...");
            let dr_ptr = gpio3_base.as_ptr().add(GPIO_SWPORT_DR_L) as *mut u32;
            dr_ptr.write_volatile(WRITE_MASK_BIT15 | 0); // Clear bit 15

            let dr_off = (gpio3_base.as_ptr().add(GPIO_SWPORT_DR_L) as *const u32).read_volatile();
            info!(
                "GPIO3 DR_L after OFF: {:#010x} (bit15={})",
                dr_off,
                (dr_off >> 15) & 1
            );

            // Wait 1 second with VBUS off
            info!("Waiting 1000ms with VBUS OFF...");
            spin_delay_ms(1000);

            // Turn ON VBUS (set GPIO3_B7 high)
            info!("Turning ON VBUS (GPIO3_B7 = 1)...");
            dr_ptr.write_volatile(WRITE_MASK_BIT15 | GPIO3_B7_BIT);

            let dr_on = (gpio3_base.as_ptr().add(GPIO_SWPORT_DR_L) as *const u32).read_volatile();
            info!(
                "GPIO3 DR_L after ON: {:#010x} (bit15={})",
                dr_on,
                (dr_on >> 15) & 1
            );

            // Wait for hub to power up and initialize
            info!("Waiting 500ms for hub to power up...");
            spin_delay_ms(500);
        }
    }

    fn try_warm_port_reset() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        unsafe {
            let caplength = (xhci_base.as_ptr().add(0x00) as *const u8).read_volatile();
            let op_base = xhci_base.as_ptr().add(caplength as usize);
            let portsc_ptr = op_base.add(0x400) as *mut u32;

            let portsc = portsc_ptr.read_volatile();
            let pls = (portsc >> 5) & 0xf;
            let ccs = portsc & 1;

            info!(
                "Before warm reset: PORTSC={:#010x}, PLS={}, CCS={}",
                portsc, pls, ccs
            );

            let pma_base_before = iomap(0xfed98000.into(), 0x10000);
            let lcpll_before_wpr =
                (pma_base_before.as_ptr().add(0x0350) as *const u32).read_volatile();
            info!(
                "PHY PLL before warm reset decision: LCPLL={:#04x} (AFC={}, LOCK={})",
                lcpll_before_wpr,
                (lcpll_before_wpr >> 6) & 1,
                (lcpll_before_wpr >> 7) & 1
            );

            if pls == 4 {
                info!("Port in Inactive state (PLS=4), attempting warm reset via WPR bit");
                let new_portsc = (portsc & 0x0e00c3e0) | (1 << 31);
                portsc_ptr.write_volatile(new_portsc);

                for _ in 0..1000000 {
                    core::hint::spin_loop();
                }

                let portsc_after = portsc_ptr.read_volatile();
                let pls_after = (portsc_after >> 5) & 0xf;
                info!(
                    "After WPR: PORTSC={:#010x}, PLS={}",
                    portsc_after, pls_after
                );
            } else if pls == 5 {
                info!("Port in RxDetect state (PLS=5), NOT doing warm reset (would kill PHY PLL)");
            } else {
                info!("Port in PLS={} state", pls);
            }

            let portsc_final = portsc_ptr.read_volatile();
            let pls_final = (portsc_final >> 5) & 0xf;
            let ccs_final = portsc_final & 1;
            info!(
                "Final: PORTSC={:#010x}, PLS={}, CCS={}",
                portsc_final, pls_final, ccs_final
            );

            let pma_base_check = iomap(0xfed98000.into(), 0x10000);
            let lcpll_after_wpr =
                (pma_base_check.as_ptr().add(0x0350) as *const u32).read_volatile();
            info!(
                "PHY PLL after warm reset: LCPLL={:#04x} (AFC={}, LOCK={})",
                lcpll_after_wpr,
                (lcpll_after_wpr >> 6) & 1,
                (lcpll_after_wpr >> 7) & 1
            );
        }
    }

    fn dump_initial_phy_state() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let pma_base = iomap(USBDPPHY1_PMA_BASE.into(), 0x10000);
        let udphygrf_base = iomap(USBDPPHY1_GRF_BASE.into(), 0x4000);
        let usbgrf_base = iomap(USB_GRF_BASE.into(), 0x1000);
        let u2phygrf_base = iomap(USB2PHY1_GRF_BASE.into(), 0x8000);
        unsafe {
            let lcpll = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
            let cdr_ln0 = (pma_base.as_ptr().add(0x0B84) as *const u32).read_volatile();
            let cdr_ln2 = (pma_base.as_ptr().add(0x1B84) as *const u32).read_volatile();
            let lane_mux = (pma_base.as_ptr().add(0x0288) as *const u32).read_volatile();
            info!(
                "PHY PMA: LCPLL={:#04x} CDR_LN0={:#04x} CDR_LN2={:#04x} LANE_MUX={:#04x}",
                lcpll, cdr_ln0, cdr_ln2, lane_mux
            );
            info!(
                "  LCPLL: AFC={} LOCK={}",
                (lcpll >> 6) & 1,
                (lcpll >> 7) & 1
            );

            let con0 = (udphygrf_base.as_ptr().add(0x00) as *const u32).read_volatile();
            let con1 = (udphygrf_base.as_ptr().add(0x04) as *const u32).read_volatile();
            info!("USBDPPHY1_GRF: CON0={:#010x} CON1={:#010x}", con0, con1);
            info!(
                "  CON1: low_pwrn={} rx_lfps={}",
                (con1 >> 13) & 1,
                (con1 >> 14) & 1
            );

            let otg1_con0 = (usbgrf_base.as_ptr().add(0x30) as *const u32).read_volatile();
            let otg1_con1 = (usbgrf_base.as_ptr().add(0x34) as *const u32).read_volatile();
            info!(
                "USB_GRF: OTG1_CON0={:#010x} OTG1_CON1={:#010x}",
                otg1_con0, otg1_con1
            );

            let grf_con2 = (u2phygrf_base.as_ptr().add(0x08) as *const u32).read_volatile();
            let grf_con4 = (u2phygrf_base.as_ptr().add(0x10) as *const u32).read_volatile();
            info!(
                "USB2PHY1_GRF: CON2={:#010x} CON4={:#010x} (bvalid regs)",
                grf_con2, grf_con4
            );

            let dwc3_base = xhci_base.as_ptr().add(DWC3_OFFSET);
            let gctl = (dwc3_base.add(0x10) as *const u32).read_volatile();
            let gusb2phycfg = (dwc3_base.add(0x100) as *const u32).read_volatile();
            let gusb3pipectl = (dwc3_base.add(0x1c0) as *const u32).read_volatile();
            info!(
                "DWC3: GCTL={:#010x} GUSB2PHYCFG={:#010x} GUSB3PIPECTL={:#010x}",
                gctl, gusb2phycfg, gusb3pipectl
            );

            let portsc = (xhci_base.as_ptr().add(0x20 + 0x400) as *const u32).read_volatile();
            info!(
                "xHCI: PORTSC={:#010x} (CCS={} PED={} PLS={})",
                portsc,
                portsc & 1,
                (portsc >> 1) & 1,
                (portsc >> 5) & 0xF
            );
        }
    }

    fn dump_usb_debug_registers() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let pma_base = iomap(USBDPPHY1_PMA_BASE.into(), 0x10000);
        let udphygrf_base = iomap(USBDPPHY1_GRF_BASE.into(), 0x4000);
        let usbgrf_base = iomap(USB_GRF_BASE.into(), 0x1000);
        let cru_base = iomap(CRU_BASE.into(), 0x1000);
        info!("=== USB DEBUG REGISTER DUMP ===");
        unsafe {
            // GATE_CON02 contains USBDP_PHY1_IMMORTAL at bit 15
            let gate_con02 = (cru_base.as_ptr().add(0x0808) as *const u32).read_volatile();
            info!("CRU GATE_CON02 (0x0808): {:#010x}", gate_con02);
            info!(
                "  USBDP_PHY0_IMMORTAL (bit8)={} USBDP_PHY1_IMMORTAL (bit15)={} (0=enabled,1=disabled)",
                (gate_con02 >> 8) & 1,
                (gate_con02 >> 15) & 1
            );

            let gate_con42 = (cru_base.as_ptr().add(0x08a8) as *const u32).read_volatile();
            let gate_con35 = (cru_base.as_ptr().add(0x088c) as *const u32).read_volatile();
            info!("CRU GATE_CON42 (0x08a8): {:#010x}", gate_con42);
            info!(
                "  aclk_usb_root={} hclk_usb_root={} aclk_usb3otg0={} aclk_usb3otg1={}",
                (gate_con42 >> 0) & 1,
                (gate_con42 >> 1) & 1,
                (gate_con42 >> 4) & 1,
                (gate_con42 >> 7) & 1
            );
            info!(
                "  ref_clk_usb3otg0={} ref_clk_usb3otg1={} suspend_clk_usb3otg1={}",
                (gate_con42 >> 6) & 1,
                (gate_con42 >> 9) & 1,
                (gate_con42 >> 8) & 1
            );
            info!("CRU GATE_CON35 (0x088c): {:#010x}", gate_con35);

            // Also check GATE_CON72 for PCLK_USBDPPHY clocks
            let gate_con72 = (cru_base.as_ptr().add(0x0920) as *const u32).read_volatile();
            info!("CRU GATE_CON72 (0x0920): {:#010x}", gate_con72);
            info!(
                "  PCLK_USBDPPHY0 (bit2)={} PCLK_USBDPPHY1 (bit4)={} (0=enabled,1=disabled)",
                (gate_con72 >> 2) & 1,
                (gate_con72 >> 4) & 1
            );
        }
        unsafe {
            let caplength = (xhci_base.as_ptr().add(0x00) as *const u8).read_volatile();
            let hcsparams1 = (xhci_base.as_ptr().add(0x04) as *const u32).read_volatile();
            let num_ports = (hcsparams1 >> 24) & 0xFF;
            info!(
                "xHCI CAPLENGTH: {:#x}, HCSPARAMS1: {:#010x} (ports={})",
                caplength, hcsparams1, num_ports
            );

            let op_base = xhci_base.as_ptr().add(caplength as usize);
            let portsc_base = op_base.add(0x400);

            for port_idx in 0..4 {
                let portsc_off = port_idx * 0x10;
                let portsc = (portsc_base.add(portsc_off) as *const u32).read_volatile();
                if portsc != 0 || port_idx < 2 {
                    info!(
                        "xHCI PORTSC[{}] (op+0x{:03x}): {:#010x}",
                        port_idx,
                        0x400 + portsc_off,
                        portsc
                    );
                    let ccs = (portsc >> 0) & 1;
                    let ped = (portsc >> 1) & 1;
                    let pp = (portsc >> 9) & 1;
                    let pls = (portsc >> 5) & 0xf;
                    let speed = (portsc >> 10) & 0xf;
                    info!(
                        "  Port {}: CCS={} PED={} PP={} PLS={} Speed={}",
                        port_idx, ccs, ped, pp, pls, speed
                    );
                }
            }

            let portsc_ss = (xhci_base.as_ptr().add(0x430) as *const u32).read_volatile();
            let portsc_usb2 = (xhci_base.as_ptr().add(0x420) as *const u32).read_volatile();
            info!("xHCI PORTSC (raw 0x430): {:#010x}", portsc_ss);
            info!("xHCI PORTSC (raw 0x420): {:#010x}", portsc_usb2);

            let ccs = (portsc_ss >> 0) & 1;
            let ped = (portsc_ss >> 1) & 1;
            let pp = (portsc_ss >> 9) & 1;
            let pls = (portsc_ss >> 5) & 0xf;
            let speed = (portsc_ss >> 10) & 0xf;
            info!(
                "  SS Port: CCS={} PED={} PP={} PLS={} Speed={}",
                ccs, ped, pp, pls, speed
            );

            let dwc3_base = xhci_base.as_ptr().add(DWC3_OFFSET);
            let gctl = (dwc3_base.add(0x10) as *const u32).read_volatile();
            let gsts = (dwc3_base.add(0x18) as *const u32).read_volatile();
            let gusb2phycfg = (dwc3_base.add(0x100) as *const u32).read_volatile();
            let gusb3pipectl = (dwc3_base.add(0x1C0) as *const u32).read_volatile();
            let gdbgltssm = (dwc3_base.add(0x64) as *const u32).read_volatile();
            info!("DWC3 GCTL: {:#010x}", gctl);
            info!("DWC3 GSTS: {:#010x}", gsts);
            info!("DWC3 GUSB2PHYCFG: {:#010x}", gusb2phycfg);
            info!("DWC3 GUSB3PIPECTL: {:#010x}", gusb3pipectl);
            info!("DWC3 GDBGLTSSM: {:#010x}", gdbgltssm);

            // DWC3 GHWPARAMS - hardware configuration registers (CRITICAL for port count!)
            // These are at offsets 0x40-0x5C from DWC3 base (not xHCI base)
            let ghwparams0 = (dwc3_base.add(0x40) as *const u32).read_volatile();
            let ghwparams1 = (dwc3_base.add(0x44) as *const u32).read_volatile();
            let ghwparams2 = (dwc3_base.add(0x48) as *const u32).read_volatile();
            let ghwparams3 = (dwc3_base.add(0x4c) as *const u32).read_volatile();
            let ghwparams4 = (dwc3_base.add(0x50) as *const u32).read_volatile();
            let ghwparams5 = (dwc3_base.add(0x54) as *const u32).read_volatile();
            let ghwparams6 = (dwc3_base.add(0x58) as *const u32).read_volatile();
            let ghwparams7 = (dwc3_base.add(0x5c) as *const u32).read_volatile();
            info!("DWC3 GHWPARAMS0: {:#010x}", ghwparams0);
            info!("DWC3 GHWPARAMS1: {:#010x}", ghwparams1);
            info!("DWC3 GHWPARAMS2: {:#010x}", ghwparams2);
            info!(
                "DWC3 GHWPARAMS3: {:#010x} (SSPHY_IFC[1:0]={}, HSPHY_IFC[3:2]={})",
                ghwparams3,
                ghwparams3 & 3,
                (ghwparams3 >> 2) & 3
            );
            info!("DWC3 GHWPARAMS4: {:#010x}", ghwparams4);
            info!("DWC3 GHWPARAMS5: {:#010x}", ghwparams5);
            info!("DWC3 GHWPARAMS6: {:#010x}", ghwparams6);
            info!(
                "DWC3 GHWPARAMS7: {:#010x} (num_hs_phy_ports[2:0]={}, num_ss_phy_ports[5:3]={})",
                ghwparams7,
                ghwparams7 & 7,
                (ghwparams7 >> 3) & 7
            );

            // Decode GHWPARAMS3 SSPHY interface
            let ssphy_ifc = ghwparams3 & 3;
            let hsphy_ifc = (ghwparams3 >> 2) & 3;
            info!(
                "  GHWPARAMS3: SSPHY_IFC={} (0=dis,1=ena), HSPHY_IFC={} (0=dis,1=utmi,2=ulpi,3=both)",
                ssphy_ifc, hsphy_ifc
            );

            let ltssm_state = gdbgltssm & 0xF;
            let ltssm_substate = (gdbgltssm >> 4) & 0xF;
            info!("  LTSSM: state={} substate={}", ltssm_state, ltssm_substate);

            let curmod = (gsts >> 0) & 0x3;
            let otg_ip = (gsts >> 20) & 1;
            let bus_err = (gsts >> 24) & 1;
            info!(
                "  GSTS: CurMode={} (0=dev,1=host,2=drd) OTG_IP={} BusErr={}",
                curmod, otg_ip, bus_err
            );

            let con0 = (udphygrf_base.as_ptr().add(0x00) as *const u32).read_volatile();
            let con1 = (udphygrf_base.as_ptr().add(0x04) as *const u32).read_volatile();
            let con2 = (udphygrf_base.as_ptr().add(0x08) as *const u32).read_volatile();
            let con3 = (udphygrf_base.as_ptr().add(0x0C) as *const u32).read_volatile();
            info!("USBDPPHY1_GRF CON0: {:#010x}", con0);
            info!("USBDPPHY1_GRF CON1: {:#010x}", con1);
            info!("USBDPPHY1_GRF CON2: {:#010x}", con2);
            info!("USBDPPHY1_GRF CON3: {:#010x}", con3);

            let low_pwrn = (con1 >> 13) & 1;
            let rx_lfps = (con1 >> 14) & 1;
            info!("  CON1: low_pwrn={} rx_lfps={}", low_pwrn, rx_lfps);

            let usb3otg1_con0 = (usbgrf_base.as_ptr().add(0x30) as *const u32).read_volatile();
            let usb3otg1_con1 = (usbgrf_base.as_ptr().add(0x34) as *const u32).read_volatile();
            info!(
                "USB_GRF USB3OTG1_CON0 (0x0030): {:#010x} (bus_filter_bypass={:#x})",
                usb3otg1_con0,
                usb3otg1_con0 & 0xF
            );
            info!("USB_GRF USB3OTG1_CON1 (0x0034): {:#010x}", usb3otg1_con1);

            let usb3otg1_status0 = (usbgrf_base.as_ptr().add(0x38) as *const u32).read_volatile();
            info!(
                "USB_GRF USB3OTG1_STATUS0 (0x0038): {:#010x}",
                usb3otg1_status0
            );

            let lcpll_done = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
            let cdr_done_ln0 = (pma_base.as_ptr().add(0x0B84) as *const u32).read_volatile();
            let cdr_done_ln2 = (pma_base.as_ptr().add(0x1B84) as *const u32).read_volatile();
            let lane_mux = (pma_base.as_ptr().add(0x0288) as *const u32).read_volatile();
            info!("USBDP PHY1 PMA LCPLL_DONE (0x0350): {:#010x}", lcpll_done);
            info!(
                "USBDP PHY1 PMA CDR_DONE LN0 (0x0B84): {:#010x}",
                cdr_done_ln0
            );
            info!(
                "USBDP PHY1 PMA CDR_DONE LN2 (0x1B84): {:#010x}",
                cdr_done_ln2
            );
            info!("USBDP PHY1 PMA LANE_MUX (0x0288): {:#010x}", lane_mux);

            let afc_done = (lcpll_done >> 6) & 1;
            let lock_done = (lcpll_done >> 7) & 1;
            info!("  LCPLL: AFC_DONE={} LOCK_DONE={}", afc_done, lock_done);

            let cr_para_con = (pma_base.as_ptr().add(0x0000) as *const u32).read_volatile();
            let dp_rstn = (pma_base.as_ptr().add(0x038C) as *const u32).read_volatile();
            info!("USBDP PHY1 PMA CR_PARA_CON (0x0000): {:#010x}", cr_para_con);
            info!("USBDP PHY1 PMA DP_RSTN (0x038C): {:#010x}", dp_rstn);

            let ln0_mon_0 = (pma_base.as_ptr().add(0x0B00) as *const u32).read_volatile();
            let ln0_mon_1 = (pma_base.as_ptr().add(0x0B04) as *const u32).read_volatile();
            let ln0_mon_2 = (pma_base.as_ptr().add(0x0B80) as *const u32).read_volatile();
            let ln1_mon_0 = (pma_base.as_ptr().add(0x1B00) as *const u32).read_volatile();
            info!("USBDP PHY1 LN0_MON (0x0B00): {:#010x}", ln0_mon_0);
            info!("USBDP PHY1 LN0_MON (0x0B04): {:#010x}", ln0_mon_1);
            info!("USBDP PHY1 LN0_MON (0x0B80): {:#010x}", ln0_mon_2);
            info!("USBDP PHY1 LN1_MON (0x1B00): {:#010x}", ln1_mon_0);

            let trsv_ln0_00 = (pma_base.as_ptr().add(0x0800) as *const u32).read_volatile();
            let trsv_ln1_00 = (pma_base.as_ptr().add(0x1000) as *const u32).read_volatile();
            info!("USBDP PHY1 TRSV_LN0 (0x0800): {:#010x}", trsv_ln0_00);
            info!("USBDP PHY1 TRSV_LN1 (0x1000): {:#010x}", trsv_ln1_00);

            let trsv_ln0_reg0206 = (pma_base.as_ptr().add(0x0818) as *const u32).read_volatile();
            let trsv_ln1_reg0406 = (pma_base.as_ptr().add(0x1018) as *const u32).read_volatile();
            let ln0_tx_drv_en = trsv_ln0_reg0206 & 1;
            let ln1_tx_drv_en = trsv_ln1_reg0406 & 1;
            info!(
                "USBDP PHY1 TRSV_LN0_REG0206 (0x0818): {:#010x} (tx_drv_idrv_en={})",
                trsv_ln0_reg0206, ln0_tx_drv_en
            );
            info!(
                "USBDP PHY1 TRSV_LN1_REG0406 (0x1018): {:#010x} (tx_drv_idrv_en={})",
                trsv_ln1_reg0406, ln1_tx_drv_en
            );

            let trsv_ln2_reg0606 = (pma_base.as_ptr().add(0x1818) as *const u32).read_volatile();
            let trsv_ln3_reg0806 = (pma_base.as_ptr().add(0x2018) as *const u32).read_volatile();
            info!(
                "USBDP PHY1 TRSV_LN2_REG0606 (0x1818): {:#010x}",
                trsv_ln2_reg0606
            );
            info!(
                "USBDP PHY1 TRSV_LN3_REG0806 (0x2018): {:#010x}",
                trsv_ln3_reg0806
            );

            let trsv_ln0_reg0000 = (pma_base.as_ptr().add(0x0800) as *const u32).read_volatile();
            let trsv_ln0_reg0002 = (pma_base.as_ptr().add(0x0808) as *const u32).read_volatile();
            let trsv_ln0_reg0003 = (pma_base.as_ptr().add(0x080C) as *const u32).read_volatile();
            info!(
                "USBDP PHY1 TRSV_LN0 (0x0800,0x0808,0x080C): {:#010x} {:#010x} {:#010x}",
                trsv_ln0_reg0000, trsv_ln0_reg0002, trsv_ln0_reg0003
            );

            let trsv_ln1_reg0200 = (pma_base.as_ptr().add(0x1000) as *const u32).read_volatile();
            let trsv_ln1_reg0202 = (pma_base.as_ptr().add(0x1008) as *const u32).read_volatile();
            let trsv_ln1_reg0203 = (pma_base.as_ptr().add(0x100C) as *const u32).read_volatile();
            info!(
                "USBDP PHY1 TRSV_LN1 (0x1000,0x1008,0x100C): {:#010x} {:#010x} {:#010x}",
                trsv_ln1_reg0200, trsv_ln1_reg0202, trsv_ln1_reg0203
            );

            let cmn_reg0060 = (pma_base.as_ptr().add(0x0180) as *const u32).read_volatile();
            let cmn_reg0063 = (pma_base.as_ptr().add(0x018C) as *const u32).read_volatile();
            let cmn_reg0064 = (pma_base.as_ptr().add(0x0190) as *const u32).read_volatile();
            info!(
                "USBDP PHY1 CMN (0x0180,0x018C,0x0190): {:#010x} {:#010x} {:#010x}",
                cmn_reg0060, cmn_reg0063, cmn_reg0064
            );

            let ln0_rx_ctle = (pma_base.as_ptr().add(0x0A00) as *const u32).read_volatile();
            let ln2_rx_ctle = (pma_base.as_ptr().add(0x1A00) as *const u32).read_volatile();
            info!("USBDP PHY1 LN0_RX_CTLE (0x0A00): {:#010x}", ln0_rx_ctle);
            info!("USBDP PHY1 LN2_RX_CTLE (0x1A00): {:#010x}", ln2_rx_ctle);

            let pipe_phy_status = (pma_base.as_ptr().add(0x0004) as *const u32).read_volatile();
            let pipe_power_present = (pma_base.as_ptr().add(0x0008) as *const u32).read_volatile();
            info!("USBDP PHY1 PMA (0x0004): {:#010x}", pipe_phy_status);
            info!("USBDP PHY1 PMA (0x0008): {:#010x}", pipe_power_present);
        }
        info!("=== END USB DEBUG REGISTER DUMP ===");
    }

    fn set_dwc3_host_mode_only() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!("DWC3 GCTL before host mode: {:#010x}", read_dwc3(GCTL));
            info!("DWC3 GUSB2PHYCFG before: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!(
                "DWC3 GUSB3PIPECTL before: {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );

            let mut gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            write_dwc3(GCTL, gctl);

            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg &= !(GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_MASK);
            phycfg |= GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            phycfg |= GUSB2PHYCFG_SUSPHY;
            write_dwc3(GUSB2PHYCFG, phycfg);

            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_DEPOCHANGE;
            pipe |= GUSB3PIPECTL_SUSPHY;
            write_dwc3(GUSB3PIPECTL, pipe);

            info!("DWC3 GCTL after host mode: {:#010x}", read_dwc3(GCTL));
            info!("DWC3 GUSB2PHYCFG after: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!("DWC3 GUSB3PIPECTL after: {:#010x}", read_dwc3(GUSB3PIPECTL));
        }
    }

    fn init_dwc3_no_phy_reset() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!(
                "DWC3 init (no reset): GCTL before = {:#010x}",
                read_dwc3(GCTL)
            );
            info!(
                "DWC3 init (no reset): GUSB2PHYCFG before = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 init (no reset): GUSB3PIPECTL before = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );

            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_DEPOCHANGE;
            pipe |= GUSB3PIPECTL_SUSPHY;
            write_dwc3(GUSB3PIPECTL, pipe);

            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg &= !(GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_MASK);
            phycfg |= GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            phycfg |= GUSB2PHYCFG_SUSPHY;
            write_dwc3(GUSB2PHYCFG, phycfg);

            let mut gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            write_dwc3(GCTL, gctl);

            info!(
                "DWC3 init (no reset): GCTL after = {:#010x}",
                read_dwc3(GCTL)
            );
            info!(
                "DWC3 init (no reset): GUSB2PHYCFG after = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 init (no reset): GUSB3PIPECTL after = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );
        }
        info!("DWC3 init (no reset) complete");
    }

    fn init_dwc3_with_soft_reset() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!(
                "DWC3 soft reset init: GCTL before = {:#010x}",
                read_dwc3(GCTL)
            );
            info!(
                "DWC3 soft reset init: GUSB2PHYCFG before = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 soft reset init: GUSB3PIPECTL before = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );

            let mut gctl = read_dwc3(GCTL);
            gctl |= GCTL_CORESOFTRESET;
            write_dwc3(GCTL, gctl);

            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe |= GUSB3PIPECTL_PHYSOFTRST;
            write_dwc3(GUSB3PIPECTL, pipe);

            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg |= GUSB2PHYCFG_PHYSOFTRST;
            write_dwc3(GUSB2PHYCFG, phycfg);

            info!("DWC3: PHY soft reset asserted, waiting 100ms...");
            spin_delay_ms(100);

            pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_PHYSOFTRST;
            write_dwc3(GUSB3PIPECTL, pipe);

            phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg &= !GUSB2PHYCFG_PHYSOFTRST;
            write_dwc3(GUSB2PHYCFG, phycfg);

            info!("DWC3: PHY soft resets cleared, waiting 100ms...");
            spin_delay_ms(100);

            gctl = read_dwc3(GCTL);
            gctl &= !GCTL_CORESOFTRESET;
            write_dwc3(GCTL, gctl);

            pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_DEPOCHANGE;
            pipe &= !GUSB3PIPECTL_SUSPHY; // Disable SUSPHY to prevent PHY lock loss
            write_dwc3(GUSB3PIPECTL, pipe);

            phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg &= !(GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_MASK);
            phycfg |= GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            phycfg &= !GUSB2PHYCFG_SUSPHY; // Disable SUSPHY to prevent PHY lock loss
            write_dwc3(GUSB2PHYCFG, phycfg);

            gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            write_dwc3(GCTL, gctl);

            info!(
                "DWC3 soft reset init: GCTL after = {:#010x}",
                read_dwc3(GCTL)
            );
            info!(
                "DWC3 soft reset init: GUSB2PHYCFG after = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 soft reset init: GUSB3PIPECTL after = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );
        }
        info!("DWC3 soft reset init complete");
    }

    fn check_port_status(label: &str) {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        unsafe {
            let caplength = (xhci_base.as_ptr().add(0x00) as *const u8).read_volatile();
            let op_base = xhci_base.as_ptr().add(caplength as usize);
            let portsc = (op_base.add(0x400) as *const u32).read_volatile();
            let ccs = portsc & 1;
            let ped = (portsc >> 1) & 1;
            let pls = (portsc >> 5) & 0xf;
            let speed = (portsc >> 10) & 0xf;
            info!(
                "Port status {}: PORTSC={:#010x} CCS={} PED={} PLS={} Speed={}",
                label, portsc, ccs, ped, pls, speed
            );
        }
    }

    fn init_dwc3_host_mode_simple() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!("DWC3 simple init: GCTL before = {:#010x}", read_dwc3(GCTL));
            info!(
                "DWC3 simple init: GUSB2PHYCFG before = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 simple init: GUSB3PIPECTL before = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );

            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_DEPOCHANGE;
            pipe |= GUSB3PIPECTL_SUSPHY;
            write_dwc3(GUSB3PIPECTL, pipe);

            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg &= !(GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_MASK);
            phycfg |= GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            phycfg |= GUSB2PHYCFG_SUSPHY;
            write_dwc3(GUSB2PHYCFG, phycfg);

            let mut gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            write_dwc3(GCTL, gctl);

            info!("DWC3 simple init: GCTL after = {:#010x}", read_dwc3(GCTL));
            info!(
                "DWC3 simple init: GUSB2PHYCFG after = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 simple init: GUSB3PIPECTL after = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );
        }
        info!("DWC3 simple init complete (no soft reset)");
    }

    fn check_phy_lock_status() {
        let pma_base = iomap(USBDPPHY1_PMA_BASE.into(), 0x10000);
        unsafe {
            let lcpll_done = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
            let cdr_done = (pma_base.as_ptr().add(0x0B84) as *const u32).read_volatile();
            let afc_done = (lcpll_done >> 6) & 1;
            let lock_done = (lcpll_done >> 7) & 1;

            info!("PHY lock status after DWC3 soft reset:");
            info!(
                "  LCPLL_DONE = {:#04x} (AFC_DONE={}, LOCK_DONE={})",
                lcpll_done, afc_done, lock_done
            );
            info!("  CDR_DONE = {:#04x}", cdr_done);

            if afc_done == 0 || lock_done == 0 {
                warn!("PHY PLL NOT LOCKED after soft reset!");
            }
        }
    }

    fn spin_delay_ms(ms: u32) {
        for _ in 0..(ms as u64 * SPIN_LOOP_PER_MS) {
            core::hint::spin_loop();
        }
    }

    fn spin_delay_us(us: u32) {
        for _ in 0..(us * SPIN_LOOP_PER_US) {
            core::hint::spin_loop();
        }
    }

    fn check_phy_lock_status_and_recover() -> bool {
        let pma_base = iomap(USBDPPHY1_PMA_BASE.into(), 0x10000);
        unsafe {
            let lcpll_done = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
            let cdr_done = (pma_base.as_ptr().add(0x0B84) as *const u32).read_volatile();
            let afc_done = (lcpll_done >> 6) & 1;
            let lock_done = (lcpll_done >> 7) & 1;

            info!(
                "PHY lock check: LCPLL_DONE={:#04x} (AFC={}, LOCK={}), CDR_DONE={:#04x}",
                lcpll_done, afc_done, lock_done, cdr_done
            );

            if afc_done == 1 && lock_done == 1 && cdr_done == 0x0f {
                info!("PHY PLL locked successfully");
                return true;
            }

            warn!("PHY PLL not locked, waiting more...");
            for retry in 0..10 {
                spin_delay_ms(100);
                let lcpll = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
                let cdr = (pma_base.as_ptr().add(0x0B84) as *const u32).read_volatile();
                let afc = (lcpll >> 6) & 1;
                let lock = (lcpll >> 7) & 1;
                info!("  Retry {}: LCPLL={:#04x} CDR={:#04x}", retry, lcpll, cdr);
                if afc == 1 && lock == 1 {
                    info!("PHY PLL locked after {} retries", retry + 1);
                    return true;
                }
            }

            false
        }
    }

    fn init_dwc3_uboot_style() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!("=== DWC3 U-Boot style init (with soft reset) ===");
            info!("GCTL before: {:#010x}", read_dwc3(GCTL));
            info!("GUSB2PHYCFG before: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!("GUSB3PIPECTL before: {:#010x}", read_dwc3(GUSB3PIPECTL));

            info!("Step 1: Assert core soft reset");
            let mut gctl = read_dwc3(GCTL);
            gctl |= GCTL_CORESOFTRESET;
            write_dwc3(GCTL, gctl);

            info!("Step 2: Assert PHY soft resets");
            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe |= GUSB3PIPECTL_PHYSOFTRST;
            write_dwc3(GUSB3PIPECTL, pipe);

            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg |= GUSB2PHYCFG_PHYSOFTRST;
            write_dwc3(GUSB2PHYCFG, phycfg);

            info!("Step 3: Wait 100ms with PHY in reset...");
            spin_delay_ms(100);

            info!("Step 4: Clear PHY soft resets");
            pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_PHYSOFTRST;
            write_dwc3(GUSB3PIPECTL, pipe);

            phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg &= !GUSB2PHYCFG_PHYSOFTRST;
            write_dwc3(GUSB2PHYCFG, phycfg);

            info!("Step 5: Wait 100ms for PHY stable...");
            spin_delay_ms(100);

            info!("Step 6: Clear core soft reset");
            gctl = read_dwc3(GCTL);
            gctl &= !GCTL_CORESOFTRESET;
            gctl &= !GCTL_DSBLCLKGTNG;
            write_dwc3(GCTL, gctl);

            info!("Step 7: Configure GUSB2PHYCFG (SUSPHY DISABLED to prevent PHY suspend)");
            phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg |= GUSB2PHYCFG_PHYIF;
            phycfg &= !GUSB2PHYCFG_USBTRDTIM_MASK;
            phycfg |= GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            phycfg &= !GUSB2PHYCFG_SUSPHY;
            write_dwc3(GUSB2PHYCFG, phycfg);

            info!("Step 8: Configure GUSB3PIPECTL (SUSPHY DISABLED to prevent PHY suspend)");
            pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_DEPOCHANGE;
            pipe &= !GUSB3PIPECTL_SUSPHY;
            write_dwc3(GUSB3PIPECTL, pipe);

            info!("Step 9: Set host mode in GCTL");
            gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            write_dwc3(GCTL, gctl);

            info!("GCTL after: {:#010x}", read_dwc3(GCTL));
            info!("GUSB2PHYCFG after: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!("GUSB3PIPECTL after: {:#010x}", read_dwc3(GUSB3PIPECTL));
            info!("=== DWC3 U-Boot style init complete ===");
        }
    }

    fn set_dwc3_host_mode_early() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            let gctl_before = read_dwc3(GCTL);
            let prtcap_before = (gctl_before >> 12) & 0x3;
            info!(
                "DWC3 early host mode: GCTL={:#010x} PRTCAPDIR={}",
                gctl_before, prtcap_before
            );

            let mut gctl = gctl_before;
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            write_dwc3(GCTL, gctl);

            let gctl_after = read_dwc3(GCTL);
            let prtcap_after = (gctl_after >> 12) & 0x3;
            info!(
                "DWC3 early host mode: GCTL={:#010x} PRTCAPDIR={}",
                gctl_after, prtcap_after
            );
        }
    }

    fn set_dwc3_host_config_no_reset() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!("=== DWC3 config with device soft reset (like U-Boot) ===");
            info!("GCTL before: {:#010x}", read_dwc3(GCTL));
            info!("GUSB2PHYCFG before: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!("GUSB3PIPECTL before: {:#010x}", read_dwc3(GUSB3PIPECTL));
            info!("DCTL before: {:#010x}", read_dwc3(DCTL));

            info!("Step 0: Device soft reset (DCTL.CSFTRST)...");
            write_dwc3(DCTL, DCTL_CSFTRST);
            let mut timeout = 5000u32;
            loop {
                let dctl = read_dwc3(DCTL);
                if (dctl & DCTL_CSFTRST) == 0 {
                    info!(
                        "Device soft reset complete after {} iterations",
                        5000 - timeout
                    );
                    break;
                }
                timeout -= 1;
                if timeout == 0 {
                    warn!("Device soft reset timeout! DCTL={:#010x}", dctl);
                    break;
                }
                spin_delay_us(1);
            }

            info!("Step 1: Configure GUSB3PIPECTL (after device reset)...");
            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_DEPOCHANGE;
            // U-Boot SETS SUSPHY for DWC3 > 1.94a (RK3588 doesn't have dis_u3_susphy_quirk)
            pipe |= GUSB3PIPECTL_SUSPHY;
            write_dwc3(GUSB3PIPECTL, pipe);

            info!("Step 2: Configure GUSB2PHYCFG...");
            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg |= GUSB2PHYCFG_PHYIF;
            phycfg &= !GUSB2PHYCFG_USBTRDTIM_MASK;
            phycfg |= GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            // U-Boot SETS SUSPHY for DWC3 > 1.94a (RK3588 doesn't have dis_u2_susphy_quirk)
            phycfg |= GUSB2PHYCFG_SUSPHY;
            write_dwc3(GUSB2PHYCFG, phycfg);

            info!("Step 3: Set host mode...");
            let mut gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            gctl &= !GCTL_DSBLCLKGTNG;
            write_dwc3(GCTL, gctl);

            info!("GCTL after: {:#010x}", read_dwc3(GCTL));
            info!("GUSB2PHYCFG after: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!("GUSB3PIPECTL after: {:#010x}", read_dwc3(GUSB3PIPECTL));
            info!("=== DWC3 config complete (with device soft reset) ===");
        }
    }

    fn set_dwc3_host_config_with_susphy() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!("=== DWC3 host config (no soft reset, SUSPHY enabled like U-Boot) ===");
            info!("GCTL before: {:#010x}", read_dwc3(GCTL));
            info!("GUSB2PHYCFG before: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!("GUSB3PIPECTL before: {:#010x}", read_dwc3(GUSB3PIPECTL));

            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg |= GUSB2PHYCFG_PHYIF;
            phycfg &= !GUSB2PHYCFG_USBTRDTIM_MASK;
            phycfg |= GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            phycfg |= GUSB2PHYCFG_SUSPHY;
            write_dwc3(GUSB2PHYCFG, phycfg);

            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_DEPOCHANGE;
            pipe |= GUSB3PIPECTL_SUSPHY;
            write_dwc3(GUSB3PIPECTL, pipe);

            let mut gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            gctl &= !GCTL_DSBLCLKGTNG;
            write_dwc3(GCTL, gctl);

            info!("GCTL after: {:#010x}", read_dwc3(GCTL));
            info!("GUSB2PHYCFG after: {:#010x}", read_dwc3(GUSB2PHYCFG));
            info!("GUSB3PIPECTL after: {:#010x}", read_dwc3(GUSB3PIPECTL));
            info!("=== DWC3 host config with SUSPHY complete ===");
        }
    }

    fn start_xhci_controller() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        // xHCI register bits
        const CMD_RUN: u32 = 1 << 0;
        const CMD_RESET: u32 = 1 << 1;
        const STS_HALT: u32 = 1 << 0;
        const STS_CNR: u32 = 1 << 11; // Controller Not Ready
        unsafe {
            let caplength = (xhci_base.as_ptr().add(0x00) as *const u8).read_volatile();
            let op_base = xhci_base.as_ptr().add(caplength as usize);

            let usbcmd_ptr = op_base.add(0x00) as *mut u32;
            let usbsts_ptr = op_base.add(0x04) as *const u32;

            let usbcmd = usbcmd_ptr.read_volatile();
            let usbsts = usbsts_ptr.read_volatile();
            info!(
                "Before reset: USBCMD={:#010x}, USBSTS={:#010x}",
                usbcmd, usbsts
            );

            // Step 1: Halt the controller if not already halted
            if (usbsts & STS_HALT) == 0 {
                info!("Halting xHCI controller...");
                let cmd = usbcmd & !CMD_RUN;
                usbcmd_ptr.write_volatile(cmd);

                // Wait for halt
                for _ in 0..1000 {
                    let sts = usbsts_ptr.read_volatile();
                    if (sts & STS_HALT) != 0 {
                        info!("xHCI halted");
                        break;
                    }
                    for _ in 0..1000 {
                        core::hint::spin_loop();
                    }
                }
            }

            // Step 2: Reset the controller (like U-Boot xhci_reset)
            info!("Resetting xHCI controller (CMD_RESET)...");
            let cmd = usbcmd_ptr.read_volatile();
            usbcmd_ptr.write_volatile(cmd | CMD_RESET);

            // Wait for reset to complete (CMD_RESET bit clears)
            for i in 0..1000 {
                let cmd = usbcmd_ptr.read_volatile();
                if (cmd & CMD_RESET) == 0 {
                    info!("xHCI reset complete after {} iterations", i);
                    break;
                }
                for _ in 0..1000 {
                    core::hint::spin_loop();
                }
            }

            // Step 3: Wait for Controller Not Ready to clear
            for i in 0..1000 {
                let sts = usbsts_ptr.read_volatile();
                if (sts & STS_CNR) == 0 {
                    info!("xHCI controller ready after {} iterations", i);
                    break;
                }
                for _ in 0..1000 {
                    core::hint::spin_loop();
                }
            }

            let usbcmd_after_reset = usbcmd_ptr.read_volatile();
            let usbsts_after_reset = usbsts_ptr.read_volatile();
            info!(
                "After reset: USBCMD={:#010x}, USBSTS={:#010x}",
                usbcmd_after_reset, usbsts_after_reset
            );

            // Check PHY PLL lock immediately after xHCI reset
            let pma_base_check = iomap(0xfed98000.into(), 0x10000);
            let lcpll_after_reset =
                (pma_base_check.as_ptr().add(0x0350) as *const u32).read_volatile();
            let cdr_after_reset =
                (pma_base_check.as_ptr().add(0x0B84) as *const u32).read_volatile();
            info!(
                "PHY PLL after xHCI reset: LCPLL={:#04x} CDR={:#04x} (AFC={}, LOCK={})",
                lcpll_after_reset,
                cdr_after_reset,
                (lcpll_after_reset >> 6) & 1,
                (lcpll_after_reset >> 7) & 1
            );

            let hcsparams1 = (xhci_base.as_ptr().add(0x04) as *const u32).read_volatile();
            let max_ports = ((hcsparams1 >> 24) & 0xff) as usize;
            info!("xHCI has {} ports, powering on all ports...", max_ports);

            const PORT_POWER: u32 = 1 << 9;
            for i in 0..max_ports {
                let portsc_offset = 0x400 + i * 0x10;
                let portsc_ptr = op_base.add(portsc_offset) as *mut u32;
                let portsc = portsc_ptr.read_volatile();
                info!(
                    "Port {} PORTSC before power: {:#010x} (PP={})",
                    i,
                    portsc,
                    (portsc >> 9) & 1
                );

                if (portsc & PORT_POWER) == 0 {
                    let new_portsc = portsc | PORT_POWER;
                    portsc_ptr.write_volatile(new_portsc);
                    info!("Port {} power enabled", i);
                }
            }

            spin_delay_ms(20);

            for i in 0..max_ports {
                let portsc_offset = 0x400 + i * 0x10;
                let portsc_ptr = op_base.add(portsc_offset) as *const u32;
                let portsc = portsc_ptr.read_volatile();
                info!(
                    "Port {} PORTSC after power: {:#010x} (PP={})",
                    i,
                    portsc,
                    (portsc >> 9) & 1
                );
            }

            info!("Starting xHCI controller (CMD_RUN)...");
            let cmd = usbcmd_ptr.read_volatile();
            usbcmd_ptr.write_volatile(cmd | CMD_RUN);

            // Wait for STS_HALT to clear
            for i in 0..100 {
                let sts = usbsts_ptr.read_volatile();
                if (sts & STS_HALT) == 0 {
                    info!("xHCI running after {} iterations! USBSTS={:#010x}", i, sts);
                    break;
                }
                for _ in 0..10000 {
                    core::hint::spin_loop();
                }
            }

            let usbcmd_after = usbcmd_ptr.read_volatile();
            let usbsts_after = usbsts_ptr.read_volatile();
            info!(
                "After start: USBCMD={:#010x}, USBSTS={:#010x}",
                usbcmd_after, usbsts_after
            );
        }
    }

    fn init_dwc3_full() {
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        let dwc3_base = unsafe { xhci_base.as_ptr().add(DWC3_OFFSET) };

        unsafe {
            let read_dwc3 =
                |off: usize| -> u32 { (dwc3_base.add(off) as *const u32).read_volatile() };
            let write_dwc3 = |off: usize, val: u32| {
                (dwc3_base.add(off) as *mut u32).write_volatile(val);
            };

            info!("DWC3 init: GCTL before = {:#010x}", read_dwc3(GCTL));
            info!(
                "DWC3 init: GUSB2PHYCFG before = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 init: GUSB3PIPECTL before = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );

            let mut gctl = read_dwc3(GCTL);
            gctl |= GCTL_CORESOFTRESET;
            write_dwc3(GCTL, gctl);

            let mut pipe = read_dwc3(GUSB3PIPECTL);
            pipe |= GUSB3PIPECTL_PHYSOFTRST;
            write_dwc3(GUSB3PIPECTL, pipe);

            let mut phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg |= GUSB2PHYCFG_PHYSOFTRST;
            write_dwc3(GUSB2PHYCFG, phycfg);

            spin_delay_ms(100);

            pipe = read_dwc3(GUSB3PIPECTL);
            pipe &= !GUSB3PIPECTL_PHYSOFTRST;
            pipe &= !GUSB3PIPECTL_DEPOCHANGE; // RK3588 quirk: dis-del-phy-power-chg-quirk
            pipe |= GUSB3PIPECTL_SUSPHY;
            write_dwc3(GUSB3PIPECTL, pipe);

            spin_delay_ms(100);

            phycfg = read_dwc3(GUSB2PHYCFG);
            phycfg &= !GUSB2PHYCFG_PHYSOFTRST;
            phycfg &= !(GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_MASK);
            phycfg |= GUSB2PHYCFG_PHYIF | GUSB2PHYCFG_USBTRDTIM_16BIT;
            phycfg &= !GUSB2PHYCFG_ENBLSLPM;
            phycfg &= !GUSB2PHYCFG_U2_FREECLK_EXISTS;
            phycfg |= GUSB2PHYCFG_SUSPHY;
            write_dwc3(GUSB2PHYCFG, phycfg);

            spin_delay_ms(100);

            gctl = read_dwc3(GCTL);
            gctl &= !GCTL_CORESOFTRESET;
            write_dwc3(GCTL, gctl);

            gctl = read_dwc3(GCTL);
            gctl &= !GCTL_PRTCAPDIR_MASK;
            gctl |= GCTL_PRTCAPDIR_HOST;
            write_dwc3(GCTL, gctl);

            info!("DWC3 init: GCTL after = {:#010x}", read_dwc3(GCTL));
            info!(
                "DWC3 init: GUSB2PHYCFG after = {:#010x}",
                read_dwc3(GUSB2PHYCFG)
            );
            info!(
                "DWC3 init: GUSB3PIPECTL after = {:#010x}",
                read_dwc3(GUSB3PIPECTL)
            );
        }
        info!("DWC3 full init complete");
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

    fn disable_vcc5v0_host(fdt: &Fdt<'static>) -> Result<(), GpioError> {
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
            let dr = base.as_ptr() as *mut u32;
            let ddr = dr.add(1);

            let mut dir = ddr.read_volatile();
            dir |= 1 << pin;
            ddr.write_volatile(dir);

            let mut val = dr.read_volatile();
            val &= !(1 << pin);
            dr.write_volatile(val);
        }

        info!(
            "vcc5v0_host DISABLED via gpio ctrl phandle 0x{:x}, pin {}",
            ctrl, pin
        );
        Ok(())
    }

    #[derive(Debug)]
    enum PhyError {
        NotFound,
        RegMissing,
    }

    #[derive(Debug)]
    enum CruError {
        NotFound,
        RegMissing,
    }

    fn init_usb2phy1_full(_fdt: &Fdt<'static>) -> Result<(), PhyError> {
        // USB2PHY1_GRF syscon base (for bvalid control in CON2/CON4)
        const USB2PHY1_GRF_SYSCON_BASE: usize = 0xfd5d4000;
        // USB2PHY1 registers are at offset 0x4000 within syscon (from device tree: usb2-phy@4000)
        const USB2PHY1_OFFSET: usize = 0x4000;
        const USB2PHY1_GRF_SIZE: usize = 0x8000;

        let grf_base = iomap(USB2PHY1_GRF_SYSCON_BASE.into(), USB2PHY1_GRF_SIZE);
        let phy_base =
            unsafe { core::ptr::NonNull::new_unchecked(grf_base.as_ptr().add(USB2PHY1_OFFSET)) };
        let cru_base = iomap(CRU_BASE.into(), 0x10000);

        unsafe {
            // PHY register access (at base + 0x4000)
            let write_phy_reg = |offset: usize, mask: u32, val: u32| {
                let ptr = phy_base.as_ptr().add(offset) as *mut u32;
                ptr.write_volatile((mask << 16) | (val & mask));
            };

            let read_phy_reg = |offset: usize| -> u32 {
                let ptr = phy_base.as_ptr().add(offset) as *const u32;
                ptr.read_volatile()
            };

            // GRF syscon register access (at base directly, for bvalid control)
            let write_grf_reg = |offset: usize, mask: u32, val: u32| {
                let ptr = grf_base.as_ptr().add(offset) as *mut u32;
                ptr.write_volatile((mask << 16) | (val & mask));
            };

            let read_grf_reg = |offset: usize| -> u32 {
                let ptr = grf_base.as_ptr().add(offset) as *const u32;
                ptr.read_volatile()
            };

            let assert_reset = |cru: core::ptr::NonNull<u8>, id: u32| {
                let (offset, bit) = if id >= 0xC0_000 {
                    let rel = id - 0xC0_000;
                    (0x30a00 + (rel / 16) * 4, rel % 16)
                } else {
                    (0xa00 + (id / 16) * 4, id % 16)
                };
                let reg = cru.as_ptr().add(offset as usize) as *mut u32;
                reg.write_volatile((1u32 << (bit + 16)) | (1u32 << bit));
            };

            let deassert_reset = |cru: core::ptr::NonNull<u8>, id: u32| {
                let (offset, bit) = if id >= 0xC0_000 {
                    let rel = id - 0xC0_000;
                    (0x30a00 + (rel / 16) * 4, rel % 16)
                } else {
                    (0xa00 + (id / 16) * 4, id % 16)
                };
                let reg = cru.as_ptr().add(offset as usize) as *mut u32;
                reg.write_volatile(1u32 << (bit + 16));
            };

            // Read PHY registers (at offset 0x4000)
            info!(
                "USB2PHY1 (phy): reg 0x0008 = {:#010x}",
                read_phy_reg(0x0008)
            );
            info!(
                "USB2PHY1 (phy): reg 0x000c = {:#010x}",
                read_phy_reg(0x000c)
            );

            // Read GRF syscon registers (at base, for bvalid)
            info!(
                "USB2PHY1 (grf): reg 0x0008 = {:#010x}",
                read_grf_reg(0x0008)
            );
            info!(
                "USB2PHY1 (grf): reg 0x0010 = {:#010x}",
                read_grf_reg(0x0010)
            );

            let siddq = read_phy_reg(0x0008);
            if (siddq & (1 << 13)) != 0 {
                info!("USB2PHY1: SIDDQ set, clearing to power on analog block");
                write_phy_reg(0x0008, 1 << 13, 0);
                for _ in 0..50000 {
                    core::hint::spin_loop();
                }

                info!("USB2PHY1: Performing PHY reset after SIDDQ clear");
                assert_reset(cru_base, 0xc0048);
                for _ in 0..10000 {
                    core::hint::spin_loop();
                }
                deassert_reset(cru_base, 0xc0048);
                for _ in 0..20000 {
                    core::hint::spin_loop();
                }
            }

            let sus_before = read_phy_reg(0x000c);
            info!(
                "USB2PHY1: phy_sus (0x000c) before = {:#010x}, bit11 = {}",
                sus_before,
                (sus_before >> 11) & 1
            );

            write_phy_reg(0x000c, 1 << 11, 0);

            for _ in 0..200000 {
                core::hint::spin_loop();
            }

            let sus_after = read_phy_reg(0x000c);
            info!(
                "USB2PHY1: phy_sus (0x000c) after = {:#010x}, bit11 = {}",
                sus_after,
                (sus_after >> 11) & 1
            );

            // Enable bvalid via software control in GRF syscon (NOT PHY regs!)
            // This is CRITICAL for device detection in USBDP combo PHY!
            // From U-Boot phy-rockchip-usbdp.c:
            //   bvalid_phy_con = { 0x0008, 1, 0, 0x2, 0x3 } - GRF CON2, bits 1-0
            //   bvalid_grf_con = { 0x0010, 3, 2, 0x2, 0x3 } - GRF CON4, bits 3-2
            //
            // CON2 bits 1-0: vbusvldextsel (bit 1) + vbusvldext (bit 0)
            //   0x3 = use external VBUS valid indicator and assert it
            // CON4 bits 3-2: sft_vbus_sel (bit 3) + sft_vbus (bit 2)
            //   0x3 = use software VBUS control and assert VBUS valid

            info!("USB2PHY1: Enabling bvalid via software control in GRF...");

            // GRF CON2 (0x0008): Set vbusvldextsel=1, vbusvldext=1 (bits 1:0 = 0x3)
            let con2_before = read_grf_reg(0x0008);
            write_grf_reg(0x0008, 0x3, 0x3);
            let con2_after = read_grf_reg(0x0008);
            info!(
                "USB2PHY1 GRF CON2 (0x0008) bvalid: {:#010x} -> {:#010x}",
                con2_before, con2_after
            );

            // GRF CON4 (0x0010): Set sft_vbus_sel=1, sft_vbus=1 (bits 3:2 = 0xC)
            let con4_before = read_grf_reg(0x0010);
            write_grf_reg(0x0010, 0xC, 0xC);
            let con4_after = read_grf_reg(0x0010);
            info!(
                "USB2PHY1 GRF CON4 (0x0010) bvalid: {:#010x} -> {:#010x}",
                con4_before, con4_after
            );

            // Wait for UTMI clock to stabilize after bvalid assertion
            for _ in 0..200000 {
                core::hint::spin_loop();
            }

            // Read STATUS0 to check actual PHY status
            let status0 = read_grf_reg(0x00C0);
            let utmi_bvalid = (status0 >> 6) & 1;
            let utmi_linestate = (status0 >> 9) & 0x3;
            let utmi_vbusvalid = (status0 >> 8) & 1;
            info!(
                "USB2PHY1 STATUS0 (grf base): {:#010x} (bvalid={}, linestate={:02b}, vbusvalid={})",
                status0, utmi_bvalid, utmi_linestate, utmi_vbusvalid
            );

            // Also read STATUS0 at PHY offset (0x4000 + 0xC0)
            let phy_status0 = read_phy_reg(0x00C0);
            let phy_bvalid = (phy_status0 >> 6) & 1;
            let phy_linestate = (phy_status0 >> 9) & 0x3;
            let phy_vbusvalid = (phy_status0 >> 8) & 1;
            info!(
                "USB2PHY1 STATUS0 (phy offset): {:#010x} (bvalid={}, linestate={:02b}, vbusvalid={})",
                phy_status0, phy_bvalid, phy_linestate, phy_vbusvalid
            );
        }

        info!("usb2phy1 fully initialized with bvalid enabled");
        Ok(())
    }
    fn force_usbdp_phy_active(_fdt: &Fdt<'static>) -> Result<(), PhyError> {
        const USBDPPHY1_GRF_SIZE: usize = 0x4000;

        let base = iomap(USBDPPHY1_GRF_BASE.into(), USBDPPHY1_GRF_SIZE);

        unsafe {
            // USBDPPHY_GRF_CON1 offset 0x04: low_pwrn (bit 13), rx_lfps (bit 14)
            let val: u32 = (((1 << 13) | (1 << 14)) << 16) | (1 << 13) | (1 << 14);
            let ptr = base.as_ptr().add(0x04) as *mut u32;
            ptr.write_volatile(val);
        }

        info!(
            "usbdpphy1-grf @ 0x{:08x} active (CON1 low_pwrn/rx_lfps set)",
            USBDPPHY1_GRF_BASE
        );
        Ok(())
    }

    /// Full USBDP PHY1 initialization for USB3_1 (Port 1)
    /// This follows the U-Boot rk3588_udphy_init sequence from phy-rockchip-usbdp.c
    fn init_usbdp_phy1_full(_fdt: &Fdt<'static>) -> Result<(), PhyError> {
        info!("=== Starting full USBDP PHY1 initialization ===");

        let pma_base = iomap(USBDPPHY1_PMA_BASE.into(), 0x10000);
        let udphygrf_base = iomap(USBDPPHY1_GRF_BASE.into(), 0x4000);
        let usbgrf_base = iomap(USB_GRF_BASE.into(), 0x1000);
        let cru_base = iomap(CRU_BASE.into(), 0x10000);

        info!("PHY1 PMA mapped at {:?}", pma_base);
        info!("USBDPPHY1_GRF mapped at {:?}", udphygrf_base);
        info!("USB_GRF mapped at {:?}", usbgrf_base);
        info!("CRU mapped at {:?}", cru_base);
        let phy_already_initialized = unsafe {
            let lcpll = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
            let cdr = (pma_base.as_ptr().add(0x0B84) as *const u32).read_volatile();
            let locked = (lcpll & 0xC0) == 0xC0 && (cdr & 0x0F) != 0;
            info!(
                "PHY lock check: LCPLL={:#04x} CDR={:#04x} already_init={}",
                lcpll, cdr, locked
            );
            locked
        };
        if phy_already_initialized {
            info!("PHY already initialized by bootloader, skipping full PHY init");
            info!("Only configuring USB_GRF to enable USB3 port...");

            unsafe {
                let con1_ptr = usbgrf_base.as_ptr().add(0x34) as *mut u32;
                let old_con1 = (usbgrf_base.as_ptr().add(0x34) as *const u32).read_volatile();
                // Write 0x1100 to enable USB3: host_num_u3_port=1, host_num_u2_port=1
                con1_ptr.write_volatile((0xFFFF << 16) | 0x1100);
                let new_con1 = (usbgrf_base.as_ptr().add(0x34) as *const u32).read_volatile();
                info!(
                    "USB3OTG1_CON1: {:#010x} -> {:#010x} (USB3 port enabled)",
                    old_con1, new_con1
                );

                let grf_con1_ptr = udphygrf_base.as_ptr().add(0x04) as *mut u32;
                let old_grf = (udphygrf_base.as_ptr().add(0x04) as *const u32).read_volatile();
                grf_con1_ptr.write_volatile(old_grf | (0x3 << 29) | (0x3 << 13));
                let new_grf = (udphygrf_base.as_ptr().add(0x04) as *const u32).read_volatile();
                info!(
                    "USBDPPHY1_GRF CON1: {:#010x} -> {:#010x} (rx_lfps, low_pwrn enabled)",
                    old_grf, new_grf
                );
            }

            info!("=== USBDP PHY1 quick config complete (bootloader-init path) ===");
            return Ok(());
        }

        let deassert_phy_reset = |cru: core::ptr::NonNull<u8>, id: u32| {
            let idx = id / 16;
            let bit = id % 16;
            let offset = 0xa00 + idx * 4;
            unsafe {
                let reg = cru.as_ptr().add(offset as usize) as *mut u32;
                // Write 1 to bit+16 (write enable) and 0 to bit (deassert)
                reg.write_volatile(1u32 << (bit + 16));
            }
        };

        let assert_phy_reset = |cru: core::ptr::NonNull<u8>, id: u32| {
            let idx = id / 16;
            let bit = id % 16;
            let offset = 0xa00 + idx * 4;
            unsafe {
                let reg = cru.as_ptr().add(offset as usize) as *mut u32;
                // Write 1 to bit+16 (write enable) and 1 to bit (assert reset)
                reg.write_volatile((1u32 << (bit + 16)) | (1u32 << bit));
            }
        };

        unsafe {
            let softrst_con02 = (cru_base.as_ptr().add(0xa08) as *const u32).read_volatile();
            let softrst_con03 = (cru_base.as_ptr().add(0xa0c) as *const u32).read_volatile();
            let softrst_con72 = (cru_base.as_ptr().add(0xb20) as *const u32).read_volatile();
            info!(
                "CRU reset state: CON02={:#010x} CON03={:#010x} CON72={:#010x}",
                softrst_con02, softrst_con03, softrst_con72
            );

            let init_reset = (softrst_con02 >> 15) & 1;
            let cmn_reset = (softrst_con03 >> 0) & 1;
            let lane_reset = (softrst_con03 >> 1) & 1;
            let pcs_reset = (softrst_con03 >> 2) & 1;
            let pma_apb_reset = (softrst_con72 >> 4) & 1;
            info!(
                "PHY1 reset bits: init={} cmn={} lane={} pcs={} pma_apb={}",
                init_reset, cmn_reset, lane_reset, pcs_reset, pma_apb_reset
            );

            let gate_con02 = (cru_base.as_ptr().add(0x0808) as *const u32).read_volatile();
            let gate_con72 = (cru_base.as_ptr().add(0x0920) as *const u32).read_volatile();
            info!(
                "Clock gates: GATE_CON02={:#010x} (IMMORTAL1 bit15={}) GATE_CON72={:#010x} (PCLK_PHY1 bit4={})",
                gate_con02,
                (gate_con02 >> 15) & 1,
                gate_con72,
                (gate_con72 >> 4) & 1
            );

            let lane_mux = (pma_base.as_ptr().add(0x0288) as *const u32).read_volatile();
            let lcpll = (pma_base.as_ptr().add(0x0350) as *const u32).read_volatile();
            let cdr = (pma_base.as_ptr().add(0x0B84) as *const u32).read_volatile();
            info!(
                "PHY state before init: LANE_MUX={:#04x} LCPLL={:#04x} CDR={:#04x}",
                lane_mux, lcpll, cdr
            );
        }
        info!("Enabling PHY clocks: USBDP_PHY1_IMMORTAL, PCLK_USBDPPHY1");
        unsafe {
            let gate_con02_ptr = cru_base.as_ptr().add(0x0808) as *mut u32;
            gate_con02_ptr.write_volatile((1u32 << 31) | (0u32 << 15));

            let gate_con72_ptr = cru_base.as_ptr().add(0x0920) as *mut u32;
            gate_con72_ptr.write_volatile((1u32 << 20) | (0u32 << 4));

            let gate_con02_after = (cru_base.as_ptr().add(0x0808) as *const u32).read_volatile();
            let gate_con72_after = (cru_base.as_ptr().add(0x0920) as *const u32).read_volatile();
            info!(
                "After clock enable: GATE_CON02={:#010x} GATE_CON72={:#010x}",
                gate_con02_after, gate_con72_after
            );
        }
        // CRITICAL: Disable USB3 port BEFORE PHY init (like U-Boot/Linux)
        // This ensures the port doesn't try to use the PHY until it's fully initialized
        // Write 0x0188 to set host_num_u3_port=0 (xHCI sees no USB3 ports)
        info!("Step 0a: DISABLE USB3 port BEFORE PHY init (CON1 = 0x0188)");
        unsafe {
            let con1_ptr = usbgrf_base.as_ptr().add(0x34) as *mut u32;
            let old_con1 = (usbgrf_base.as_ptr().add(0x34) as *const u32).read_volatile();
            con1_ptr.write_volatile((0xFFFF << 16) | 0x0188);
            let new_con1 = (usbgrf_base.as_ptr().add(0x34) as *const u32).read_volatile();
            info!(
                "USB3OTG1_CON1: {:#010x} -> {:#010x} (USB3 port DISABLED for PHY init)",
                old_con1, new_con1
            );
        }
        info!("Step 0b: Asserting ALL PHY resets");
        assert_phy_reset(cru_base, 47); // init
        assert_phy_reset(cru_base, 48); // cmn
        assert_phy_reset(cru_base, 49); // lane
        assert_phy_reset(cru_base, 50); // pcs_apb
        assert_phy_reset(cru_base, 1156); // pma_apb
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
        unsafe {
            let softrst_con02 = (cru_base.as_ptr().add(0xa08) as *const u32).read_volatile();
            let softrst_con03 = (cru_base.as_ptr().add(0xa0c) as *const u32).read_volatile();
            let softrst_con72 = (cru_base.as_ptr().add(0xb20) as *const u32).read_volatile();
            info!(
                "After assert: CON02={:#010x} CON03={:#010x} CON72={:#010x}",
                softrst_con02, softrst_con03, softrst_con72
            );
        }

        info!("Step 1: Enable rx_lfps for USB mode");
        unsafe {
            let ptr = udphygrf_base.as_ptr().add(0x04) as *mut u32;
            let val: u32 = ((1 << 14) << 16) | (1 << 14);
            ptr.write_volatile(val);
        }

        info!("Step 2: Power on PMA (low_pwrn=1)");
        unsafe {
            let ptr = udphygrf_base.as_ptr().add(0x04) as *mut u32;
            let val: u32 = ((1 << 13) << 16) | (1 << 13);
            ptr.write_volatile(val);
        }

        info!("Step 3: Deassert pma_apb and pcs_apb resets");
        deassert_phy_reset(cru_base, 1156);
        deassert_phy_reset(cru_base, 50);

        // Step 4: Write init sequence to PMA
        // From U-Boot rk3588_udphy_init_sequence
        const INIT_SEQUENCE: &[(u16, u8)] = &[
            (0x0104, 0x44),
            (0x0234, 0xE8),
            (0x0248, 0x44),
            (0x028C, 0x18),
            (0x081C, 0xE5),
            (0x0878, 0x00),
            (0x0994, 0x1C),
            (0x0AF0, 0x00),
            (0x181C, 0xE5),
            (0x1878, 0x00),
            (0x1994, 0x1C),
            (0x1AF0, 0x00),
            (0x0428, 0x60),
            (0x0D58, 0x33),
            (0x1D58, 0x33),
            (0x0990, 0x74),
            (0x0D64, 0x17),
            (0x08C8, 0x13),
            (0x1990, 0x74),
            (0x1D64, 0x17),
            (0x18C8, 0x13),
            (0x0D90, 0x40),
            (0x0DA8, 0x40),
            (0x0DC0, 0x40),
            (0x0DD8, 0x40),
            (0x1D90, 0x40),
            (0x1DA8, 0x40),
            (0x1DC0, 0x40),
            (0x1DD8, 0x40),
            (0x03C0, 0x30),
            (0x03C4, 0x06),
            (0x0E10, 0x00),
            (0x1E10, 0x00),
            (0x043C, 0x0F),
            (0x0D2C, 0xFF),
            (0x1D2C, 0xFF),
            (0x0D34, 0x0F),
            (0x1D34, 0x0F),
            (0x08FC, 0x2A),
            (0x0914, 0x28),
            (0x0A30, 0x03),
            (0x0E38, 0x05),
            (0x0ECC, 0x27),
            (0x0ED0, 0x22),
            (0x0ED4, 0x26),
            (0x18FC, 0x2A),
            (0x1914, 0x28),
            (0x1A30, 0x03),
            (0x1E38, 0x05),
            (0x1ECC, 0x27),
            (0x1ED0, 0x22),
            (0x1ED4, 0x26),
            (0x0048, 0x0F),
            (0x0060, 0x3C),
            (0x0064, 0xF7),
            (0x006C, 0x20),
            (0x0070, 0x7D),
            (0x0074, 0x68),
            (0x0AF4, 0x1A),
            (0x1AF4, 0x1A),
            (0x0440, 0x3F),
            (0x10D4, 0x08),
            (0x20D4, 0x08),
            (0x00D4, 0x30),
            (0x0024, 0x6e),
        ];

        unsafe {
            for &(offset, value) in INIT_SEQUENCE {
                let ptr = pma_base.as_ptr().add(offset as usize) as *mut u32;
                ptr.write_volatile(value as u32);
            }
        }
        info!(
            "Step 4: Init sequence written ({} registers)",
            INIT_SEQUENCE.len()
        );

        // Step 5: Write 24MHz reference clock configuration
        const REFCLK_CFG: &[(u16, u8)] = &[
            (0x0090, 0x68),
            (0x0094, 0x68),
            (0x0128, 0x24),
            (0x012c, 0x44),
            (0x0130, 0x3f),
            (0x0134, 0x44),
            (0x015c, 0xa9),
            (0x0160, 0x71),
            (0x0164, 0x71),
            (0x0168, 0xa9),
            (0x0174, 0xa9),
            (0x0178, 0x71),
            (0x017c, 0x71),
            (0x0180, 0xa9),
            (0x018c, 0x41),
            (0x0190, 0x00),
            (0x0194, 0x05),
            (0x01ac, 0x2a),
            (0x01b0, 0x17),
            (0x01b4, 0x17),
            (0x01b8, 0x2a),
            (0x01c8, 0x04),
            (0x01cc, 0x08),
            (0x01d0, 0x08),
            (0x01d4, 0x04),
            (0x01d8, 0x20),
            (0x01dc, 0x01),
            (0x01e0, 0x09),
            (0x01e4, 0x03),
            (0x01f0, 0x29),
            (0x01f4, 0x02),
            (0x01f8, 0x02),
            (0x01fc, 0x29),
            (0x0208, 0x2a),
            (0x020c, 0x17),
            (0x0210, 0x17),
            (0x0214, 0x2a),
            (0x0224, 0x20),
            (0x03f0, 0x0d),
            (0x03f4, 0x09),
            (0x03f8, 0x09),
            (0x03fc, 0x0d),
            (0x0404, 0x0e),
            (0x0408, 0x14),
            (0x040c, 0x14),
            (0x0410, 0x3b),
            (0x0ce0, 0x68),
            (0x0ce8, 0xd0),
            (0x0cf0, 0x87),
            (0x0cf8, 0x70),
            (0x0d00, 0x70),
            (0x0d08, 0xa9),
            (0x1ce0, 0x68),
            (0x1ce8, 0xd0),
            (0x1cf0, 0x87),
            (0x1cf8, 0x70),
            (0x1d00, 0x70),
            (0x1d08, 0xa9),
            (0x0a3c, 0xd0),
            (0x0a44, 0xd0),
            (0x0a48, 0x01),
            (0x0a4c, 0x0d),
            (0x0a54, 0xe0),
            (0x0a5c, 0xe0),
            (0x0a64, 0xa8),
            (0x1a3c, 0xd0),
            (0x1a44, 0xd0),
            (0x1a48, 0x01),
            (0x1a4c, 0x0d),
            (0x1a54, 0xe0),
            (0x1a5c, 0xe0),
            (0x1a64, 0xa8),
        ];

        unsafe {
            for &(offset, value) in REFCLK_CFG {
                let ptr = pma_base.as_ptr().add(offset as usize) as *mut u32;
                ptr.write_volatile(value as u32);
            }
        }
        info!(
            "Step 5: Refclk config written ({} registers)",
            REFCLK_CFG.len()
        );

        // Step 6: Configure lane mux - ALL lanes USB mode (matching U-Boot working config)
        // Note: DTS has rockchip,dp-lane-mux = <2 3> but U-Boot's usb start uses 0x00 (all USB)
        // The GL3523 hub is connected to USB lanes, not DP lanes
        info!("Step 6: Configure lane mux - ALL lanes USB mode (match U-Boot)");
        unsafe {
            let ptr = pma_base.as_ptr().add(0x0288) as *mut u32;
            let current = ptr.read_volatile();
            let mask: u32 = 0xFF;
            let value: u32 = 0x00;
            let new_val = (current & !mask) | (value & mask);
            ptr.write_volatile(new_val);
            info!(
                "Lane mux: current=0x{:02x}, new=0x{:02x} (all USB)",
                current, new_val
            );
        }

        info!("Step 7: Deassert init reset (USB mode)");
        deassert_phy_reset(cru_base, 47);

        for _ in 0..1000 {
            core::hint::spin_loop();
        }
        info!("Step 8: Deassert cmn and lane resets (USB mode)");
        deassert_phy_reset(cru_base, 48);
        deassert_phy_reset(cru_base, 49);

        info!("Step 9: Waiting for LCPLL lock...");
        let mut timeout = 500; // 500 iterations
        loop {
            let val = unsafe {
                let ptr = pma_base.as_ptr().add(0x0350) as *const u32;
                ptr.read_volatile()
            };
            let afc_done = (val & (1 << 6)) != 0;
            let lock_done = (val & (1 << 7)) != 0;
            if afc_done && lock_done {
                info!("LCPLL locked! val=0x{:02x}", val);
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                warn!(
                    "LCPLL lock timeout! val=0x{:02x} (AFC={}, LOCK={})",
                    val, afc_done, lock_done
                );
                break;
            }
            for _ in 0..10000 {
                core::hint::spin_loop();
            }
        }

        // Step 7: Wait for CDR lock (Lane 0 for non-flipped)
        // TRSV_LN0_MON_RX_CDR_DONE offset 0x0B84, LOCK_DONE = bit 0
        info!("Step 10: Waiting for CDR lock...");
        timeout = 500;
        loop {
            let val = unsafe {
                let ptr = pma_base.as_ptr().add(0x0B84) as *const u32;
                ptr.read_volatile()
            };
            if (val & 1) != 0 {
                info!("CDR locked! val=0x{:02x}", val);
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                warn!("CDR lock timeout! val=0x{:02x} - continuing anyway", val);
                break;
            }
            for _ in 0..10000 {
                core::hint::spin_loop();
            }
        }

        // Step 10: Read USB_GRF_USB3OTG1_CON0 (offset 0x0030) - don't modify
        unsafe {
            let ptr = usbgrf_base.as_ptr().add(0x0030) as *const u32;
            let con0 = ptr.read_volatile();
            info!(
                "Step 10: USB3OTG1_CON0: {:#010x} (bus_filter_bypass={:#x}) - NOT modifying",
                con0,
                con0 & 0xF
            );
        }

        // Step 11: ALWAYS enable USB3 after PHY init completes
        // Write 0x1100: host_num_u3_port=1, host_num_u2_port=1 (USB3 mode)
        unsafe {
            let ptr = usbgrf_base.as_ptr().add(0x0034) as *mut u32;
            let current_con1 = (usbgrf_base.as_ptr().add(0x0034) as *const u32).read_volatile();
            let val: u32 = (0xFFFF << 16) | 0x1100;
            ptr.write_volatile(val);
            let con1_after = (usbgrf_base.as_ptr().add(0x0034) as *const u32).read_volatile();
            info!(
                "Step 11: ENABLE USB3 port (CON1: {:#010x} -> {:#010x})",
                current_con1, con1_after
            );
        }

        info!("=== USBDP PHY1 initialization complete ===");
        Ok(())
    }

    fn reinit_usbdp_phy1_after_reset(_fdt: &Fdt<'static>) -> Result<(), PhyError> {
        info!("=== Re-initializing USBDP PHY1 after DWC3 soft reset ===");

        let pma_base = iomap(USBDPPHY1_PMA_BASE.into(), 0x10000);
        let udphygrf_base = iomap(USBDPPHY1_GRF_BASE.into(), 0x4000);
        let cru_base = iomap(CRU_BASE.into(), 0x10000);

        let deassert_phy_reset = |cru: core::ptr::NonNull<u8>, id: u32| {
            let idx = id / 16;
            let bit = id % 16;
            let offset = 0xa00 + idx * 4;
            unsafe {
                let reg = cru.as_ptr().add(offset as usize) as *mut u32;
                reg.write_volatile(1u32 << (bit + 16));
            }
        };

        unsafe {
            let ptr = udphygrf_base.as_ptr().add(0x04) as *mut u32;
            let val: u32 = ((1 << 14) << 16) | (1 << 14);
            ptr.write_volatile(val);
        }
        info!("Re-enabled rx_lfps");

        unsafe {
            let ptr = udphygrf_base.as_ptr().add(0x04) as *mut u32;
            let val: u32 = ((1 << 13) << 16) | (1 << 13);
            ptr.write_volatile(val);
        }
        info!("Re-enabled low_pwrn");

        deassert_phy_reset(cru_base, 1156);
        deassert_phy_reset(cru_base, 50);
        deassert_phy_reset(cru_base, 47);
        deassert_phy_reset(cru_base, 48);
        deassert_phy_reset(cru_base, 49);
        info!("Deasserted PHY resets");

        info!("Waiting for PHY to re-lock...");
        let mut timeout = 1000;
        loop {
            let val = unsafe {
                let ptr = pma_base.as_ptr().add(0x0350) as *const u32;
                ptr.read_volatile()
            };
            let afc_done = (val & (1 << 6)) != 0;
            let lock_done = (val & (1 << 7)) != 0;
            if afc_done && lock_done {
                info!("PHY re-locked! LCPLL=0x{:02x}", val);
                break;
            }
            timeout -= 1;
            if timeout == 0 {
                warn!("PHY re-lock timeout! LCPLL=0x{:02x}", val);
                break;
            }
            for _ in 0..10000 {
                core::hint::spin_loop();
            }
        }

        let cdr_val = unsafe {
            let ptr = pma_base.as_ptr().add(0x0B84) as *const u32;
            ptr.read_volatile()
        };
        info!("CDR status: 0x{:02x}", cdr_val);

        info!("=== USBDP PHY1 re-initialization complete ===");
        Ok(())
    }

    /// 解除 USB 控制器、UTMI、PHY 等相关复位。参照 rk3588-cru.h 中的复位编号。
    ///
    /// 目前只做“写掩码 + 清零”操作，不额外断言/拉高。
    fn deassert_rk3588_usb_resets(fdt: &Fdt<'static>) -> Result<(), CruError> {
        // 定位 CRU 基址
        let Some(cru_node) = fdt
            .all_nodes()
            .find(|n| n.compatibles().any(|c| c.contains("rk3588-cru")))
        else {
            return Err(CruError::NotFound);
        };

        let mut regs = cru_node.reg().ok_or(CruError::RegMissing)?;
        let reg = regs.next().ok_or(CruError::RegMissing)?;
        let cru_base = iomap((reg.address as usize).into(), reg.size.unwrap_or(0x1000));

        // 需要释放的 reset ID，参考 include/dt-bindings/clock/rk3588-cru.h
        // USB1 对应 xHCI @ 0xfc400000
        const RESET_IDS: &[u32] = &[
            674,    // SRST_A_USB_BIU
            675,    // SRST_H_USB_BIU
            676,    // SRST_A_USB3OTG0 (一并释放，不影响)
            679,    // SRST_A_USB3OTG1 (目标控制器)
            682,    // SRST_H_HOST0
            683,    // SRST_H_HOST_ARB0
            684,    // SRST_H_HOST1
            685,    // SRST_H_HOST_ARB1
            686,    // SRST_A_USB_GRF
            688,    // SRST_C_USB2P0_HOST1
            689,    // SRST_HOST_UTMI0
            690,    // SRST_HOST_UTMI1
            1153,   // SRST_P_USBDPGRF0
            1154,   // SRST_P_USBDPPHY0
            1155,   // SRST_P_USBDPGRF1
            1156,   // SRST_P_USBDPPHY1
            1161,   // SRST_P_USB2PHY_U3_1_GRF0
            1163,   // SRST_P_USB2PHY_U2_1_GRF0
            1175,   // SRST_USBDP_COMBO_PHY1
            1176,   // SRST_USBDP_COMBO_PHY1_LCPLL
            1177,   // SRST_USBDP_COMBO_PHY1_ROPLL
            1178,   // SRST_USBDP_COMBO_PHY1_PCS_HS
            786503, // SRST_OTGPHY_U3_0 (宽松释放)
            786504, // SRST_OTGPHY_U3_1
            786505, // SRST_OTGPHY_U2_0
            786506, // SRST_OTGPHY_U2_1
        ];

        for &id in RESET_IDS {
            deassert_one_reset(cru_base, id);
        }

        Ok(())
    }

    /// 根据 reset ID 计算寄存器偏移并写入 deassert。
    fn deassert_one_reset(cru_base: core::ptr::NonNull<u8>, id: u32) {
        // RK3588：常规 reset（CRU）采用 id/16 对应 SOFTRST_CON(N)；
        // PMU1 reset 的 id 以 0xC0_000 起始，对应 PMU1SOFTRST_CON。
        let (offset, bit) = if id >= 0xC0_000 {
            let rel = id - 0xC0_000;
            let idx = rel / 16;
            let bit = rel % 16;
            // PMU1SOFTRST_CON00 offset = 0x30a00，后续按 4 字节递增
            (0x30a00 + idx * 4, bit)
        } else {
            let idx = id / 16;
            let bit = id % 16;
            // SOFTRST_CON00 offset = 0xa00，后续按 4 字节递增
            (0xa00 + idx * 4, bit)
        };

        unsafe {
            let reg = cru_base.as_ptr().add(offset as usize) as *mut u32;
            // hi16 作为写掩码，低 16 位写 0 解除 reset
            reg.write_volatile(1u32 << (bit + 16));
        }
    }

    /// Toggle VBUS power with configurable timing
    /// Returns true if device connected after toggle
    fn try_vbus_gpio_toggle_timed(off_ms: u32, on_wait_ms: u32) -> bool {
        let gpio3_base = iomap(GPIO3_BASE.into(), 0x1000);
        let xhci_base = iomap(USB3_1_BASE.into(), 0x10000);
        unsafe {
            // Ensure GPIO3_B7 is configured as output
            let ddr_ptr = gpio3_base.as_ptr().add(GPIO_SWPORT_DDR_L) as *mut u32;
            ddr_ptr.write_volatile(WRITE_MASK_BIT15 | GPIO3_B7_BIT);

            // Turn OFF VBUS
            let dr_ptr = gpio3_base.as_ptr().add(GPIO_SWPORT_DR_L) as *mut u32;
            dr_ptr.write_volatile(WRITE_MASK_BIT15 | 0);

            spin_delay_ms(off_ms);

            // Turn ON VBUS
            dr_ptr.write_volatile(WRITE_MASK_BIT15 | GPIO3_B7_BIT);

            spin_delay_ms(on_wait_ms);

            // Check if device connected
            let caplength = (xhci_base.as_ptr().add(0x00) as *const u8).read_volatile();
            let op_base = xhci_base.as_ptr().add(caplength as usize);
            let portsc = (op_base.add(0x410) as *const u32).read_volatile(); // Port 1 (USB3)
            let ccs = portsc & 1;
            ccs == 1
        }
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
