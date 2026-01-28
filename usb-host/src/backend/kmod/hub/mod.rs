pub mod device;

use core::any::Any;
use core::fmt::Debug;

use alloc::boxed::Box;
use alloc::collections::btree_map::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use futures::future::BoxFuture;
use usb_if::err::USBError;
use usb_if::host::hub::Speed;
// 重新导出常用类型
pub use device::{HubDevice, PortState};
use id_arena::Id;

pub trait HubOp: Send + 'static + Any {
    fn init<'a>(&'a mut self, info: HubInfo) -> BoxFuture<'a, Result<HubInfo, USBError>>;
    fn changed_ports<'a>(&'a mut self) -> BoxFuture<'a, Result<Vec<PortChangeInfo>, USBError>>;
    fn slot_id(&self) -> u8;
}

#[derive(Debug, Clone)]
pub struct PortChangeInfo {
    pub root_port_id: u8,
    pub port_id: u8,
    pub port_speed: Speed,
    /// 设备在 Hub 上的端口号（如果需要 Transaction Translator）
    pub tt_port_on_hub: Option<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RouteString(u32);

impl RouteString {
    /// 创建新的 Route String
    pub fn follow_root() -> Self {
        Self(0)
    }

    /// 获取 Route String 的原始值
    pub fn raw(&self) -> u32 {
        self.0
    }

    pub fn push_hub(&mut self, hub_port: u8) {
        assert!(hub_port <= 15);
        let mut target_depth = None;
        for depth in 1..=5 {
            let shift = (depth - 1) * 4;
            let port = (self.0 >> shift) & 0x0F;
            if port == 0 {
                target_depth = Some(depth);
                break;
            }
        }

        let depth = target_depth.expect("route string is full");
        let shift = (depth - 1) * 4;
        let mask = 0x0F << shift;
        self.0 = (self.0 & !mask) | (((hub_port as u32) & 0x0F) << shift);
    }

    pub fn route_port_ids(&self) -> impl Iterator<Item = u8> + '_ {
        (0..5).filter_map(move |depth| {
            let port = ((self.0 >> (depth * 4)) & 0x0F) as u8;
            if port == 0 { None } else { Some(port) }
        })
    }

    pub fn to_port_path_string(self, root_hub_port: u8) -> String {
        let mut parts = vec![root_hub_port.to_string()];
        for port in self.route_port_ids() {
            parts.push(port.to_string());
        }
        parts.join(".")
    }
}

impl Debug for RouteString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut iter = self.route_port_ids();
        if let Some(first) = iter.next() {
            write!(f, "{first}")?;
            for port in iter {
                write!(f, ".{port}")?;
            }
        }
        Ok(())
    }
}

pub struct Hub {
    pub info: HubInfo,
    pub backend: Box<dyn HubOp>,
}
impl Hub {
    pub fn new(
        backend: Box<dyn HubOp>,
        infos: &BTreeMap<Id<Hub>, HubInfo>,
        port_id: u8,
        parent: Option<Id<Hub>>,
    ) -> Self {
        let slot_id;
        let mut hub_depth = 0;
        if parent.is_none() {
            hub_depth = -1;
            slot_id = 0;
        } else {
            slot_id = backend.slot_id();
            let mut current_parent = parent;
            while let Some(p) = current_parent {
                let parent = infos.get(&p).expect("parent hub info must exist");
                if parent.hub_depth == -1 {
                    break;
                }

                hub_depth += 1;
                current_parent = infos.get(&p).and_then(|info| info.parent);
            }
        }

        Self {
            backend,
            info: HubInfo {
                parent,
                port_id,
                slot_id,
                hub_depth,
                speed: Speed::Full,
                tt: UsbTt {
                    multi: false,
                    think_time_ns: 0,
                },
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct HubInfo {
    /// 若为 None, 则表示 Root Hub
    pub parent: Option<Id<Hub>>,
    pub slot_id: u8,
    pub hub_depth: isize,
    pub speed: Speed,
    pub port_id: u8,
    pub tt: UsbTt,
}

#[derive(Debug, Clone, Copy)]
pub struct UsbTt {
    multi: bool,
    think_time_ns: usize,
}

#[cfg(test)]
mod tests {

    use super::RouteString;

    #[test]
    fn test_route_string() {
        let mut rs = RouteString::follow_root();
        rs.push_hub(3);
        rs.push_hub(5);
        rs.push_hub(2);
        assert_eq!(rs.raw(), 0b0010_0101_0011);
        assert_eq!(format!("{:?}", rs), "3.5.2");
        println!("raw: {:#x}", rs.0);
    }
}
