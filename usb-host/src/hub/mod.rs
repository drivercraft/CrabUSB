//! USB Hub 管理模块
//!
//! 提供 Root Hub 和 External Hub 的管理功能，包括：
//! - Hub 设备发现和枚举
//! - 端口状态管理
//! - 设备连接/断开事件处理
//! - 多级 Hub 嵌套支持
//!
//! # 架构
//!
//! ```text
//! HubManager
//!     ├── HubDevice (Root Hub / External Hub)
//!     │   ├── Port[] (端口列表)
//!     │   └── State (Hub 状态)
//!     └── Event Queue (无锁事件队列)
//! ```

pub mod device;
pub mod event;
pub mod manager;

// 重新导出常用类型
pub use device::{HubDevice, HubState, Port, PortState};
pub use event::{HubEvent, HubEventHandler, HubId, HubStatusChange};
pub use manager::HubManager;
