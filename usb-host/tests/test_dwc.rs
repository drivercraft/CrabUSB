#![no_std]
#![no_main]
#![feature(used_with_arg)]

use bare_test::driver::register;
use rdrive::{Phandle, PlatformDevice, probe::OnProbeError, register::FdtInfo};

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
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use crab_usb::{impl_trait, *};
use futures::{FutureExt, future::BoxFuture};
use log::info;
use log::*;
use pcie::*;
use rockchip_pm::{PowerDomain, RockchipPM};
use rockchip_soc::rk3588::Cru;
use spin::Mutex;
use usb_if::{
    descriptor::{ConfigurationDescriptor, EndpointType},
    transfer::Direction,
};

#[bare_test::tests]
mod tests {

    use core::ptr::NonNull;

    use bare_test::time::spin_delay;

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

            fn delay(duration: Duration) {
                spin_delay(duration);
            }
        }
    }

    struct XhciInfo {
        usb: USBHost<Dwc>,
        irq: Option<IrqInfo>,
    }

    fn get_usb_host() -> XhciInfo {
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

                let phy_phandle = Phandle::from(0x495);

                let u3_phy_node = fdt
                    .get_node_by_phandle(phy_phandle)
                    .expect("Failed to find u3phy node");

                info!("Found phy node: {}", u3_phy_node.name());

                let u3phy_reg = u3_phy_node.reg().unwrap().collect::<Vec<_>>().remove(0);

                let phy = iomap(
                    (u3phy_reg.address as usize).into(),
                    u3phy_reg.size.unwrap_or(0x1000),
                );

                let u2phy_grf = get_grf(
                    u3_phy_node
                        .find_property("rockchip,u2phy-grf")
                        .unwrap()
                        .u32()
                        .into(),
                );

                let usb_grf = get_grf(
                    u3_phy_node
                        .find_property("rockchip,usb-grf")
                        .unwrap()
                        .u32()
                        .into(),
                );

                let usbdpphy_grf = get_grf(
                    u3_phy_node
                        .find_property("rockchip,usbdpphy-grf")
                        .unwrap()
                        .u32()
                        .into(),
                );

                let vo_grf = get_grf(
                    u3_phy_node
                        .find_property("rockchip,vo-grf")
                        .unwrap()
                        .u32()
                        .into(),
                );

                let dp_lane_mux_prop = u3_phy_node
                    .find_property("rockchip,dp-lane-mux")
                    .expect("Missing rockchip,dp-lane-mux property");

                let dp_lane_mux = dp_lane_mux_prop.u32_list().collect::<Vec<_>>();
                let mut rst_list = Vec::new();
                let resets_prop = u3_phy_node
                    .find_property("resets")
                    .expect("Missing resets property");
                let resets = resets_prop.u32_list().collect::<Vec<_>>();
                let reset_names_prop = u3_phy_node
                    .find_property("reset-names")
                    .expect("Missing reset-names property");
                let reset_names = reset_names_prop.str_list().collect::<Vec<_>>();
                for (cell, &name) in resets.chunks(2).zip(reset_names.iter()) {
                    rst_list.push((name, cell[1] as u64));
                }

                return XhciInfo {
                    usb: USBHost::new_dwc(
                        addr,
                        phy,
                        UdphyParam {
                            u2phy_grf,
                            usb_grf,
                            usbdpphy_grf,
                            vo_grf,
                            dp_lane_mux: &dp_lane_mux,
                            rst_list: &rst_list,
                        },
                        CruOpImpl,
                        u32::MAX as usize,
                    )
                    .unwrap(),
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

        info!("RockchipPM initialized at {:p}", mmio.as_ptr());

        // 开启 USB 电源域 (domain 31)
        // 这是 usbdrd3_0 和 usbdrd3_1 控制器所需的电源域
        match pm.power_domain_on(rockchip_pm::PowerDomain(31)) {
            Ok(_) => info!("USB power domain (31) enabled successfully"),
            Err(e) => {
                error!("Failed to enable USB power domain: {:?}", e);
                panic!("USB power domain enable failed");
            }
        }

        // 可选：开启 PHP 总线结构域 (domain 32)
        // PHP 是处理高性能总线的电源域，某些 USB 配置可能需要
        match pm.power_domain_on(rockchip_pm::PowerDomain(32)) {
            Ok(_) => info!("PHP power domain (32) enabled successfully"),
            Err(e) => {
                warn!(
                    "Failed to enable PHP power domain: {:?} (may be optional)",
                    e
                );
            }
        }

        info!("All required power domains enabled");
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
}

fn get_grf(phandle: Phandle) -> NonNull<u8> {
    let PlatformInfoKind::DeviceTree(fdt) = &global_val().platform_info;
    let fdt = fdt.get();

    let node = fdt.get_node_by_phandle(phandle).unwrap();

    info!("Found node: {}", node.name());

    let regs = node.reg().unwrap().collect::<Vec<_>>();
    let reg = regs[0];
    iomap((reg.address as usize).into(), reg.size.unwrap_or(0x1000))
}

rdrive::module_driver! {
    name: "CRU",
    level: ProbeLevel::PostKernel,
    priority: ProbePriority::CLK,
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

    let grf_phandle = node
        .node
        .find_property("rockchip,grf")
        .unwrap()
        .u32()
        .into();

    let grf = get_grf(grf_phandle);

    let clk = CruDev(Cru::new(base, grf));

    dev.register(clk);

    Ok(())
}

struct CruDev(Cru);

impl rdrive::DriverGeneric for CruDev {
    fn open(&mut self) -> Result<(), rdrive::KError> {
        Ok(())
    }

    fn close(&mut self) -> Result<(), rdrive::KError> {
        Ok(())
    }
}

struct CruOpImpl;

impl CruOp for CruOpImpl {
    fn reset_assert(&self, id: u64) {
        let cru = rdrive::get_list::<CruDev>().remove(0);
        cru.lock().unwrap().0.reset_assert(id.into());
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
