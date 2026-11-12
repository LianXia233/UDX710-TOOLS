//! 认证、授权、限流、CORS 与安全响应头（报告 08 章）。
//!
//! 关键修复：
//! - **移除 `/api/device_control` 的 POST/OPTIONS 免认证白名单**；
//! - `OPTIONS` 只返回 CORS 预检响应，绝不进入业务 Handler；
//! - `/api/auth/password`、`/api/auth/logout` 由原先「绕过 Token 校验」改为必须认证；
//! - 所有写操作默认要求 Token，并按 `permission_for` 做细粒度授权；
//! - 登录失败限速在 `AuthService`，此处再叠加全局请求限速；
//! - CSP 不含 `unsafe-eval`，插件模板不启用运行时 JS 编译。
//!
//! WebSocket 认证说明：浏览器 `WebSocket` / `EventSource` 无法设置 `Authorization` 头，
//! 前端把会话 token 作为 `?token=` 附加。本项目**仅在** `/api/ws`、`/api/terminal/ws`
//! 的 WS 升级握手（必须带 `Upgrade: websocket`）与 `/api/events` 的 SSE 降级请求上接受
//! query token；普通 REST 请求一律只认 header token，避免把凭据散落在任意 URL 里。
//! 配套的 `request_log` 只记录 path、绝不记录 query / header / body，防止令牌进入日志。

use crate::response::ApiError;
use crate::AppState;
use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use udx_domain::error::AppError;
use udx_domain::permissions::permission_for;

/// 每分钟每 IP 的简单固定窗口计数器；有上限、无外部依赖。
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<HashMap<String, (u64, u32)>>>,
    limit: u32,
}

impl RateLimiter {
    pub fn new(limit: u32) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            limit: limit.max(1),
        }
    }

    pub fn check(&self, key: &str) -> bool {
        let bucket = udx_storage::now_ts() as u64 / 60;
        let Ok(mut map) = self.inner.lock() else {
            return true;
        };
        // 清理过期窗口，避免 HashMap 无限增长（报告 4.7 有限窗口）。
        map.retain(|_, (b, _)| *b >= bucket.saturating_sub(1));
        let entry = map.entry(key.to_string()).or_insert((bucket, 0));
        if entry.0 != bucket {
            *entry = (bucket, 0);
        }
        if entry.1 >= self.limit {
            return false;
        }
        entry.1 += 1;
        true
    }
}

/// 公开路由白名单。注意：设备控制类写操作一律不在其中。
fn is_public(method: &Method, path: &str) -> bool {
    let m = method.as_str();
    matches!(
        (m, path),
        ("POST", "/api/auth/login")
            | ("GET", "/api/auth/status")
            | ("GET", "/api/info")
            | ("GET", "/api/charge/config")
            | ("GET", "/api/current_band")
            | ("GET", "/api/security/questions")
            | ("POST", "/api/security/verify")
            | ("POST", "/api/security/reset-password")
            | ("GET", "/api/health")
            | ("GET", "/api/events")
    )
}

/// 长连接 handler 用于在会话内复核凭证的原始 Token。
/// 只存在于请求扩展中，绝不写入任何日志或响应。
#[derive(Clone)]
pub struct SessionToken(pub String);

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let raw = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    raw.strip_prefix("Bearer ").map(str::trim).filter(|s| !s.is_empty())
}

/// 允许通过 query 传递会话 token 的实时通道路径（且仅限这些路径）。
const STREAM_PATHS: [&str; 3] = ["/api/ws", "/api/terminal/ws", "/api/events"];
const MAX_QUERY_BYTES: usize = 4096;
const MAX_TOKEN_BYTES: usize = 256;

/// 极简 percent-decode：token 由 `encodeURIComponent` 生成，字符集有限，无需引入额外依赖。
fn percent_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let src = s.as_bytes();
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i] == b'%' && i + 2 < src.len() {
            if let (Some(h), Some(l)) = (hex(src[i + 1]), hex(src[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(if src[i] == b'+' { b' ' } else { src[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 仅在浏览器无法设置 Authorization 头的实时通道握手上读取 `?token=`。
///
/// - WS 必须携带 `Upgrade: websocket`，否则视同普通 HTTP 请求（不接受 query token）；
/// - SSE 是普通 GET，无 Upgrade 头，按路径白名单放行；
/// - 其他任何路径（含 `/api/shell`）都不接受 query token；
/// - query / token 长度设上限，避免异常输入放大开销。
fn query_token(req: &Request<axum::body::Body>) -> Option<String> {
    let path = req.uri().path();
    if !STREAM_PATHS.contains(&path) {
        return None;
    }
    if path != "/api/events" {
        let upgrade = req.headers().get(header::UPGRADE)?.to_str().ok()?;
        if !upgrade.eq_ignore_ascii_case("websocket") {
            return None;
        }
    }
    let query = req.uri().query()?;
    if query.len() > MAX_QUERY_BYTES {
        return None;
    }
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == "token" && !v.is_empty() && v.len() <= MAX_TOKEN_BYTES {
            let decoded = percent_decode(v);
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    None
}

/// 主闸门中间件：CORS 预检 → 限速 → 认证 → 细粒度授权。
pub async fn gate(
    State(state): State<AppState>,
    mut req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    // OPTIONS 只处理预检，不执行任何业务动作（安全审计项）。
    if method == Method::OPTIONS {
        return preflight_response();
    }

    let client = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let rate_key = format!("{client}::{path}");
    if !state.rate_limit.check(&rate_key) {
        return ApiError::from(AppError::RateLimited).into_response();
    }

    if !is_public(&method, &path) {
        // header token 优先；仅实时通道握手允许回退到 query token。
        // 转成 String 是为了在 await 之后仍可安全地取得 req 的可变借用。
        let token = bearer_token(req.headers())
            .map(str::to_string)
            .or_else(|| query_token(&req))
            .unwrap_or_default();
        let principal = match state.services.auth.verify(&token).await {
            Ok(p) => p,
            Err(e) => return ApiError::from(e).into_response(),
        };
        if let Some(permission) = permission_for(method.as_str(), &path) {
            if !principal.can(permission) {
                return ApiError::from(AppError::Forbidden(permission)).into_response();
            }
        }
        req.extensions_mut().insert(principal);
        // 供 WS 长会话逐帧复核；仅存内存，不进日志/响应。
        req.extensions_mut().insert(SessionToken(token));
    }

    next.run(req).await
}

fn preflight_response() -> Response {
    (
        StatusCode::NO_CONTENT,
        [
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, PUT, DELETE, OPTIONS",
            ),
            (
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                "Authorization, Content-Type",
            ),
            (header::ACCESS_CONTROL_MAX_AGE, "600"),
        ],
    )
        .into_response()
}

/// 访问日志：只记录 method / path / status / 耗时。
///
/// 刻意**不记录 query**（WS 会话 token 走 `?token=`）、不记录请求头、不记录 body；
/// 替代 `TraceLayer::new_for_http()` 的 `DefaultMakeSpan`（它会把完整 URI 写入 span）。
pub async fn request_log(req: Request<axum::body::Body>, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let started = Instant::now();
    let response = next.run(req).await;
    let status = response.status().as_u16();
    tracing::info!(
        method = %method,
        path = %path,
        status = status,
        latency_ms = started.elapsed().as_millis(),
        "http"
    );
    response
}

/// 安全响应头：CSP 不含 unsafe-eval / unsafe-inline。
pub async fn security_headers(req: Request<axum::body::Body>, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            // 与 web/index.html 的 CSP meta 保持一致（frame-ancestors 仅响应头有效）：
            // 两份策略同时生效时取交集，避免 header 漏掉 font-src / form-action。
            "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
             font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'self'; \
             form-action 'self'; frame-ancestors 'none'",
        ),
    );
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    h.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    // CORS：设备控制台通常同源访问；跨域时仍允许预检与读取，
    // 但认证与权限闸门不因此放宽。
    h.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS"),
    );
    h.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type"),
    );
    resp
}

/// 供集成测试使用：验证公开白名单不含设备控制、query token 作用域受控。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_control_is_not_public() {
        assert!(!is_public(&Method::POST, "/api/device_control"));
        assert!(!is_public(&Method::OPTIONS, "/api/device_control"));
        assert!(!is_public(&Method::POST, "/api/shell"));
        assert!(!is_public(&Method::POST, "/api/auth/password"));
    }

    #[test]
    fn required_public_routes_stay_public() {
        assert!(is_public(&Method::POST, "/api/auth/login"));
        assert!(is_public(&Method::GET, "/api/auth/status"));
        assert!(is_public(&Method::GET, "/api/health"));
    }

    #[test]
    fn permission_mapping_covers_high_risk_actions() {
        assert_eq!(
            permission_for("POST", "/api/device_control"),
            Some(udx_domain::perms::DEVICE_CONTROL)
        );
        assert_eq!(
            permission_for("POST", "/api/shell"),
            Some(udx_domain::perms::SHELL_EXECUTE)
        );
    }

    fn ws_request(path: &str, query: &str, upgrade: Option<&str>) -> Request<axum::body::Body> {
        let mut b = Request::builder().method(Method::GET).uri(format!("{path}{query}"));
        if let Some(u) = upgrade {
            b = b.header(header::UPGRADE, u).header(header::CONNECTION, "Upgrade");
        }
        b.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn query_token_only_on_ws_upgrade_or_sse() {
        // 正常的 WS 升级握手：读取 token
        assert_eq!(
            query_token(&ws_request("/api/terminal/ws", "?token=abc123", Some("websocket")))
                .as_deref(),
            Some("abc123")
        );
        assert_eq!(
            query_token(&ws_request("/api/ws", "?token=xyz", Some("WebSocket"))).as_deref(),
            Some("xyz")
        );
        // percent-decode
        assert_eq!(
            query_token(&ws_request("/api/ws", "?token=a%2Fb%3Dc", Some("websocket"))).as_deref(),
            Some("a/b=c")
        );
        // 普通 GET（无 Upgrade 头）不接受 query token
        assert_eq!(query_token(&ws_request("/api/terminal/ws", "?token=abc123", None)), None);
        // Upgrade 头不是 websocket 时不接受
        assert_eq!(
            query_token(&ws_request("/api/ws", "?token=abc", Some("h2c"))),
            None
        );
        // 非实时通道路径一律不接受（尤其 /api/shell）
        assert_eq!(
            query_token(&ws_request("/api/shell", "?token=abc", Some("websocket"))),
            None
        );
        // SSE 降级通道是普通 GET，按白名单放行
        assert_eq!(query_token(&ws_request("/api/events", "?token=abc", None)).as_deref(), Some("abc"));
        // 超长 token 拒绝
        let long = "x".repeat(MAX_TOKEN_BYTES + 1);
        assert_eq!(
            query_token(&ws_request("/api/ws", &format!("?token={long}"), Some("websocket"))),
            None
        );
    }
}
