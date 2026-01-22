pub mod device;

use core::fmt::Debug;

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use usb_if::host::hub::DeviceSpeed;
// 重新导出常用类型
pub use device::{HubDevice, PortState};
use id_arena::Id;

#[derive(Debug, Clone)]
pub struct PortChangeInfo {
    pub root_port_id: u8,
    pub port_id: u8,
    pub port_speed: DeviceSpeed,
    /// 设备在 Hub 上的端口号（如果需要 Transaction Translator）
    pub tt_port_on_hub: Option<u8>,
    /// Parent Hub 是否支持 Multi-TT
    pub parent_hub_multi_tt: bool,
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

    /// Return the deepest (last non-zero) port id in the route string.
    pub fn last_port(&self) -> Option<u8> {
        self.route_port_ids().last()
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
    pub parent: Option<Id<Hub>>,
    /// Roothub 为 `None`
    pub route_string: Option<RouteString>,
    pub backend: Box<dyn crate::backend::ty::HubOp>,
}
impl Hub {
    pub fn new(
        backend: Box<dyn crate::backend::ty::HubOp>,
        route_string: Option<RouteString>,
    ) -> Self {
        Self {
            route_string,
            backend,
            parent: None,
        }
    }
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
