//! 统一错误语义。
//!
//! 设计要点（报告 05/08 章）：
//! - 内部错误与外部 API 错误分离：内部细节只写入结构化日志，不回显给客户端；
//! - 错误体保持旧版 `{ "status": "error", "message": "..." }` 语义；
//! - 每个设备动作都有明确的成功 / 失败 / 超时 / 未知状态，不返回模糊结果。

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 对外错误类别，决定 HTTP 状态码与稳定错误码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    BadRequest,
    Validation,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    Conflict,
    Busy,
    Timeout,
    TooLarge,
    RateLimited,
    DeviceUnavailable,
    ServiceUnavailable,
    Internal,
}

impl ErrorKind {
    pub fn status(self) -> u16 {
        match self {
            Self::BadRequest | Self::Validation => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
            Self::Conflict | Self::Busy => 409,
            Self::Timeout => 504,
            Self::TooLarge => 413,
            Self::RateLimited => 429,
            Self::DeviceUnavailable | Self::ServiceUnavailable => 503,
            Self::Internal => 500,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::Validation => "validation_failed",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::Conflict => "conflict",
            Self::Busy => "busy",
            Self::Timeout => "timeout",
            Self::TooLarge => "payload_too_large",
            Self::RateLimited => "rate_limited",
            Self::DeviceUnavailable => "device_unavailable",
            Self::ServiceUnavailable => "service_unavailable",
            Self::Internal => "internal_error",
        }
    }
}

/// 与旧版兼容的错误响应体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    pub status: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<&'static str>,
}

impl ErrorBody {
    pub fn new(message: impl Into<String>, kind: ErrorKind) -> Self {
        Self {
            status: "error",
            message: message.into(),
            code: Some(kind.code()),
        }
    }
}

/// 应用内部错误。外部响应由 [`AppError::public_message`] 统一脱敏。
#[derive(Debug, Error)]
pub enum AppError {
    #[error("请求参数错误: {0}")]
    BadRequest(String),

    #[error("输入校验失败: {0}")]
    Validation(String),

    #[error("未认证")]
    Unauthorized,

    #[error("权限不足: 需要 {0}")]
    Forbidden(&'static str),

    #[error("资源不存在: {0}")]
    NotFound(String),

    #[error("方法不允许")]
    MethodNotAllowed,

    #[error("资源冲突: {0}")]
    Conflict(String),

    #[error("设备资源忙: {0}")]
    Busy(String),

    #[error("操作超时: {0}")]
    Timeout(String),

    #[error("负载过大: {0}")]
    TooLarge(String),

    #[error("请求过于频繁")]
    RateLimited,

    #[error("设备不可用: {0}")]
    DeviceUnavailable(String),

    #[error("依赖服务不可用: {0}")]
    ServiceUnavailable(String),

    #[error("内部错误: {0}")]
    Internal(String),
}

impl AppError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::BadRequest(_) => ErrorKind::BadRequest,
            Self::Validation(_) => ErrorKind::Validation,
            Self::Unauthorized => ErrorKind::Unauthorized,
            Self::Forbidden(_) => ErrorKind::Forbidden,
            Self::NotFound(_) => ErrorKind::NotFound,
            Self::MethodNotAllowed => ErrorKind::MethodNotAllowed,
            Self::Conflict(_) => ErrorKind::Conflict,
            Self::Busy(_) => ErrorKind::Busy,
            Self::Timeout(_) => ErrorKind::Timeout,
            Self::TooLarge(_) => ErrorKind::TooLarge,
            Self::RateLimited => ErrorKind::RateLimited,
            Self::DeviceUnavailable(_) => ErrorKind::DeviceUnavailable,
            Self::ServiceUnavailable(_) => ErrorKind::ServiceUnavailable,
            Self::Internal(_) => ErrorKind::Internal,
        }
    }

    pub fn status(&self) -> u16 {
        self.kind().status()
    }

    /// 对外消息：内部错误不泄漏路径、命令、SQL 或堆栈。
    pub fn public_message(&self) -> String {
        match self {
            Self::Internal(_) | Self::ServiceUnavailable(_) => {
                "服务暂时不可用，请稍后重试".to_string()
            }
            Self::Unauthorized => "未授权，请先登录".to_string(),
            Self::Forbidden(p) => format!("权限不足：需要 {p}"),
            Self::BadRequest(m)
            | Self::Validation(m)
            | Self::NotFound(m)
            | Self::Conflict(m)
            | Self::Busy(m)
            | Self::Timeout(m)
            | Self::TooLarge(m)
            | Self::DeviceUnavailable(m) => m.clone(),
            Self::MethodNotAllowed => "方法不允许".to_string(),
            Self::RateLimited => "请求过于频繁，请稍后重试".to_string(),
        }
    }

    pub fn body(&self) -> ErrorBody {
        ErrorBody::new(self.public_message(), self.kind())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// 便捷构造。
pub fn bad_request(msg: impl Into<String>) -> AppError {
    AppError::BadRequest(msg.into())
}
pub fn internal(msg: impl Into<String>) -> AppError {
    AppError::Internal(msg.into())
}
pub fn unavailable(msg: impl Into<String>) -> AppError {
    AppError::DeviceUnavailable(msg.into())
}
