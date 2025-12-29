#![no_std]
#![no_main]
#![feature(used_with_arg)]

use bare_test::driver::register;
use rdrive::{PlatformDevice, probe::OnProbeError, register::FdtInfo};

extern crate alloc;

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
use rk3588_clk::Rk3588Cru;
use rockchip_pm::{PowerDomain, RockchipPM};
use usb_if::{
    descriptor::{ConfigurationDescriptor, EndpointType},
    transfer::Direction,
};

#[bare_test::tests]
mod tests {

    use core::ptr::NonNull;

    use super::*;

    static PROT_CHANGED: AtomicBool = AtomicBool::new(false);

    #[test]
    fn test_all() {
        // enable_clk();
        enable_power();

        spin_on::spin_on(async {
            let info = get_usb_host();
            let irq_info = info.irq.clone().unwrap();

            let mut host = Box::pin(info.usb);

            register_irq(irq_info, &mut host);

            host.init().await.unwrap();
            info!("usb host init ok");
            info!("usb cmd test");

            for _ in 0..10 {
                if PROT_CHANGED.load(Ordering::Acquire) {
                    info!("port change detected");
                    PROT_CHANGED.store(false, Ordering::Release);
                    break;
                }
                sleep(Duration::from_millis(100)).await;
            }

            let mut ls = Vec::new();
            for _ in 0..3 {
                let ls2 = host.probe_devices().await.unwrap();
                if ls2.len() > 0 {
                    info!("found {} devices", ls2.len());
                    ls = ls2;
                    break;
                }
                sleep(Duration::from_millis(1000)).await;
            }

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
                let interface_desc = interface_desc.unwrap();
                let config_desc = config_desc.unwrap();

                let mut device = host.open_device(&info).await.unwrap();

                info!("open device ok: {device:?}");

                device
                    .set_configuration(config_desc.configuration_value)
                    .await
                    .unwrap();
                info!("set configuration ok");

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
        usb: USBHost<Dwc>,
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
                        usb: USBHost::new_dwc(addr, u32::MAX as usize).unwrap(),
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

            if node.compatibles().any(|c| c.contains("snps,dwc3")) {
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

                for clk in node.clocks() {
                    println!("usb clock: {:?}", clk);
                }

                // ensure_rk3588_usb_power(&fdt, &node);

                // preper_3588_clk(&fdt, &node);

                let addr = iomap(
                    (regs[0].address as usize).into(),
                    regs[0].size.unwrap_or(0x1000),
                );

                let irq = node.irq_info();

                return XhciInfo {
                    usb: USBHost::new_dwc(addr, u32::MAX as usize).unwrap(),
                    irq,
                };
            }
        }

        panic!("no xhci found");
    }

    fn register_irq(irq: IrqInfo, host: &mut USBHost<Dwc>) {
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

    fn enable_power() {
        let mmio = get_syscon_addr();
        let mut pm = RockchipPM::new(mmio, rockchip_pm::RkBoard::Rk3588);

        
    }
    fn get_syscon_addr() -> NonNull<u8> {
        let PlatformInfoKind::DeviceTree(fdt) = &global_val().platform_info;
        let fdt = fdt.get();

        let node = fdt
            .find_compatible(&["syscon"])
            .find(|n| n.name().contains("power-manage"))
            .expect("Failed to find syscon node");

        info!("Found node: {}", node.name());

        let regs = node.reg().unwrap().collect::<Vec<_>>();
        let start = regs[0].address as usize;
        let end = start + regs[0].size.unwrap_or(0);
        info!("Syscon address range: 0x{:x} - 0x{:x}", start, end);
        let start = start & !(page_size() - 1);
        let end = (end + page_size() - 1) & !(page_size() - 1);
        info!("Aligned Syscon address range: 0x{:x} - 0x{:x}", start, end);
        iomap(start.into(), end - start)
    }

    /// 打开 RK3588 USB 电源域（如果设备树有 power-domains 描述）
    fn ensure_rk3588_usb_power(fdt: &Fdt<'static>, usb_node: &Node<'static>) {
        // let power_prop = match usb_node.find_property("power-domains") {
        //     Some(p) => p,
        //     None => {
        //         debug!(
        //             "{} has no power-domains, skip PMU power on",
        //             usb_node.name()
        //         );
        //         return;
        //     }
        // };

        // let clks = usb_node.clocks().collect::<Vec<_>>();

        // for clk in clks {
        //     debug!("usb clock: {:?}", clk);
        // }

        // let mut ls = power_prop.u32_list();
        // let ctrl_phandle = match ls.next() {
        //     Some(v) => v,
        //     None => return,
        // };
        // let mut domains: Vec<u32> = ls.collect();
        // if domains.is_empty() {
        //     debug!(
        //         "{} power-domains has no domain IDs, skip PMU power on",
        //         usb_node.name()
        //     );
        //     return;
        // }

        // debug!(
        //     "power-domains for {}: ctrl=0x{:x}, domains={:?}",
        //     usb_node.name(),
        //     ctrl_phandle,
        //     domains
        // );

        // // 精确查找 PMU syscon 基址：compatible 必须等于 "rockchip,rk3588-pmu"
        // let pmu_node = fdt
        //     .all_nodes()
        //     .find(|n| n.compatibles().any(|c| c == "rockchip,rk3588-pmu"))
        //     .or_else(|| {
        //         // 兜底：按照 power-domains ctrl phandle 找 power-controller 本身
        //         fdt.get_node_by_phandle(ctrl_phandle.into())
        //     });

        // let Some(pmu_node) = pmu_node else {
        //     warn!("rk3588 pmu node not found, skip powering usb domain");
        //     return;
        // };

        // let mut regs = match pmu_node.reg() {
        //     Some(r) => r,
        //     None => {
        //         warn!("pmu node without reg, skip powering usb domain");
        //         return;
        //     }
        // };

        // let Some(reg) = regs.next() else {
        //     warn!("pmu node reg empty, skip powering usb domain");
        //     return;
        // };

        // let start = (reg.address as usize) & !(page_size() - 1);
        // let end = (reg.address as usize + reg.size.unwrap_or(0x1000) + page_size() - 1)
        //     & !(page_size() - 1);
        // let base = iomap(start.into(), end - start);

        // let compatible = pmu_node
        //     .compatibles()
        //     .find(|c| {
        //         matches!(
        //             *c,
        //             "rockchip,rk3588-power-controller" | "rockchip,rk3568-power-controller"
        //         )
        //     })
        //     .unwrap_or("rockchip,rk3588-power-controller");

        // let mut pm = RockchipPM::new_with_compatible(base, compatible);

        // // Power on domains described by DT, then ensure PHP (bus fabric) is on.
        // let mut pd_list: Vec<PowerDomain> =
        //     domains.iter().map(|id| PowerDomain::from(*id)).collect();

        // if let Some(php_pd) = pm.get_power_dowain_by_name("php") {
        //     // Avoid duplicates if DT already lists PHP.
        //     if !pd_list.contains(&php_pd) {
        //         pd_list.push(php_pd);
        //     }
        // }

        // for pd in pd_list {
        //     if let Err(e) = pm.power_domain_on(pd) {
        //         warn!("enable {:?} power domain failed: {e:?}", pd);
        //     } else {
        //         info!("enabled rk3588 power domain {:?}", pd);
        //     }
        // }

        // // 使能 VBUS 5V (vcc5v0_host) GPIO，如果存在的话
        // if let Err(e) = enable_vcc5v0_host(fdt) {
        //     warn!("enable vcc5v0_host gpio failed: {:?}", e);
        // }

        // // 解除 USB2 PHY suspend，确保 UTMI 时钟与上拉打开
        // if let Err(e) = force_usb2phy_active(fdt) {
        //     warn!("force usb2phy active failed: {:?}", e);
        // }

        // // 解除 USB3 DP Combo PHY 电源/电气休眠，防止 PIPE 侧被关断
        // if let Err(e) = force_usbdp_phy_active(fdt) {
        //     warn!("force usbdp phy active failed: {:?}", e);
        // }

        // // 释放 XHCI/PHY 相关复位，避免控制器或 PHY 仍处于 reset 状态
        // if let Err(e) = deassert_rk3588_usb_resets(fdt) {
        //     warn!("deassert usb resets failed: {:?}", e);
        // }
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

rdrive::module_driver! {
    name: "CRU",
    level: ProbeLevel::PostKernel,
    priority: ProbePriority::DEFAULT,
    probe_kinds: &[ProbeKind::Fdt {
        compatibles: &["rockchip,rk3588-cru"],
        // Use `probe_clk` above; this usage is because doctests cannot find the parent module.
        on_probe: on_probe_cru,
    }],
}

fn on_probe_cru(node: FdtInfo<'_>, dev: PlatformDevice) -> Result<(), OnProbeError> {
    // Initialization code for CRU can be added here if needed.
    // 获取 CRU 寄存器基址
    let Some(reg) = node.node.reg().and_then(|mut r| r.next()) else {
        warn!("CRU node has no valid register, skip clock enable");
        return Err(OnProbeError::KError(rdrive::KError::BadAddr(0)));
    };

    let base = iomap((reg.address as usize).into(), reg.size.unwrap_or(0x1000));

    info!("RK3588 CRU base at {:p}", base.as_ptr());

    let clk = Rk3588Cru::new(base);

    dev.register(Cru(clk));

    Ok(())
}

struct Cru(Rk3588Cru);

unsafe impl Send for Cru {}
unsafe impl Sync for Cru {}

impl rdrive::DriverGeneric for Cru {
    fn open(&mut self) -> Result<(), rdrive::KError> {
        Ok(())
    }

    fn close(&mut self) -> Result<(), rdrive::KError> {
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
