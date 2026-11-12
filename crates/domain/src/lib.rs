//! UDX710-TOOLS 领域层：模型、错误、状态与权限。
//!
//! 该 crate 不依赖任何具体框架（Axum / Tokio / SQLite），只定义
//! 「设备状态、错误语义、任务状态与业务规则」，供上层复用。

pub mod error;
pub mod models;
pub mod permissions;
pub mod state;

pub use error::{bad_request, internal, unavailable, AppError, AppResult, ErrorBody, ErrorKind};
pub use permissions::{permission_for, perms, Principal, Role};
pub use state::{
    ActionOutcome, AtPriority, DeviceActionRecord, NetworkConfigDomain, TaskState, UsbMode,
    UsbSwitchPhase, UsbSwitchState,
};
