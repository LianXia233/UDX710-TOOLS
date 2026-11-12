//! 统一响应与错误适配。
//!
//! `AppError` 定义在 domain crate，`IntoResponse` 定义在 axum；
//! 两者都是外部类型，因此用本地 newtype `ApiError` 适配（避免孤儿规则问题）。
//! 内部错误只写日志，不回显堆栈、SQL、命令或路径。

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::Value;
use udx_domain::error::{AppError, ErrorBody};

pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = self.0;
        if err.status() >= 500 {
            tracing::error!(target: "api", kind = ?err.kind(), "内部错误: {}", err.to_string());
        }
        let status = StatusCode::from_u16(err.status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body: ErrorBody = err.body();
        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// 成功响应：直接透传 JSON，保持旧版字段语义。
pub fn ok(value: Value) -> ApiResult<Json<Value>> {
    Ok(Json(value))
}

/// 便捷构造：把任意 `AppResult<Value>` 转成 ApiResult。
pub fn wrap(result: udx_domain::error::AppResult<Value>) -> ApiResult<Json<Value>> {
    result.map(Json).map_err(ApiError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_errors_are_not_echoed_to_client() {
        let e = AppError::Internal("SELECT * FROM users WHERE token='secret'".into());
        assert_eq!(e.status(), 500);
        assert!(!e.public_message().contains("SELECT"));
        assert_eq!(e.body().status, "error");
    }
}
