//! Axum API 层：路由、认证/授权中间件、静态资源、WebSocket/SSE。
//!
//! 分层边界（报告 03 章）：Handler 只处理协议、认证、请求校验与响应格式，
//! 复杂硬件动作全部委托给 `udx-services`。

pub mod handlers;
pub mod middleware;
pub mod response;
pub mod routes;
pub mod static_files;
pub mod ws;

use axum::Router;
use std::sync::Arc;
use udx_services::AppServices;

/// 请求级共享状态。Clone 廉价（全是 Arc / 通道）。
#[derive(Clone)]
pub struct AppState {
    pub services: Arc<AppServices>,
    pub rate_limit: middleware::RateLimiter,
}

impl AppState {
    pub fn new(services: Arc<AppServices>) -> Self {
        let rate_limit =
            middleware::RateLimiter::new(services.config.limits.rate_limit_per_minute);
        Self {
            services,
            rate_limit,
        }
    }
}

/// 构建完整 Router（API + 内嵌静态资源 + 安全中间件）。
pub fn build_router(state: AppState) -> Router {
    routes::router(state)
}
