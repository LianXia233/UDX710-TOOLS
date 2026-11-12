//! API 集成测试：真实 Router + 认证中间件 + 服务层（报告 11 章验收）。
//!
//! 这些测试不需要 UDX710 真机：设备侧硬件动作会返回明确的降级结果，
//! 但认证、授权、注入防护、静态资源与 **Web 任意 Shell** 都可端到端验证。

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use udx_api::{build_router, AppState};
use udx_services::{AppConfig, AppServices};

async fn test_app() -> (Router, Arc<AppServices>) {
    let mut cfg = AppConfig::default();
    cfg.database = ":memory:".to_string();
    cfg.dbus.call_timeout_secs = 1;
    let services = AppServices::new(cfg).await.expect("服务初始化应成功");
    let app = build_router(AppState::new(services.clone()));
    (app, services)
}

fn req(method: Method, uri: &str, body: Option<Value>, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(t) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let payload = body
        .map(|v| Body::from(v.to_string()))
        .unwrap_or_else(Body::empty);
    builder.body(payload).unwrap()
}

async fn call(app: &Router, request: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
    let resp = app.clone().oneshot(request).await.expect("router 应可调用");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), 4 * 1024 * 1024)
        .await
        .expect("响应体可读");
    let value: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        json!({ "_raw": String::from_utf8_lossy(&bytes).into_owned() })
    });
    (status, headers, value)
}

async fn login(app: &Router) -> String {
    let (status, _, body) = call(
        app,
        req(
            Method::POST,
            "/api/auth/login",
            Some(json!({"password": "admin"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "登录应成功: {body}");
    body.get("token")
        .and_then(|v| v.as_str())
        .expect("响应应包含 token")
        .to_string()
}

#[tokio::test]
async fn health_and_info_are_public() {
    let (app, _) = test_app().await;
    for path in ["/api/health", "/api/info", "/api/auth/status"] {
        let (status, _, body) = call(&app, req(Method::GET, path, None, None)).await;
        assert_eq!(status, StatusCode::OK, "{path} 应公开可访问: {body}");
    }
}

#[tokio::test]
async fn device_control_is_no_longer_whitelisted() {
    let (app, _) = test_app().await;
    // 报告 08 章严重项：旧版 POST /api/device_control 免认证，必须修复。
    let (status, _, body) = call(
        &app,
        req(
            Method::POST,
            "/api/device_control",
            Some(json!({"action": "reboot"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未认证必须拒绝: {body}");
}

#[tokio::test]
async fn options_is_preflight_only_and_never_executes_device_actions() {
    let (app, _) = test_app().await;
    let token = login(&app).await;

    let (status, _, _) = call(&app, req(Method::OPTIONS, "/api/device_control", None, None)).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "OPTIONS 仅返回预检");

    // 审计表中不应出现 device_control 动作（预检没有触发业务 Handler）。
    let (status, _, body) = call(
        &app,
        req(Method::GET, "/api/audit/logs", None, Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = body.to_string();
    assert!(!text.contains("device_control"), "预检触发了设备动作: {text}");
}

#[tokio::test]
async fn password_change_and_logout_require_authentication() {
    let (app, _) = test_app().await;
    for path in ["/api/auth/password", "/api/auth/logout"] {
        let (status, _, _) = call(
            &app,
            req(Method::POST, path, Some(json!({})), None),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} 必须认证");
    }
}

#[tokio::test]
async fn web_shell_is_preserved_but_authenticated() {
    // 用户明确要求保留 Web 任意 Shell；这里验证它真的能执行任意命令，
    // 同时必须带有效管理员 Token。
    let (app, _) = test_app().await;

    let (status, _, _) = call(
        &app,
        req(
            Method::POST,
            "/api/shell",
            Some(json!({"command": "printf udx-shell-ok"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "无 Token 必须拒绝 Web Shell");

    let token = login(&app).await;
    let (status, _, body) = call(
        &app,
        req(
            Method::POST,
            "/api/shell",
            Some(json!({"command": "printf udx-shell-ok"})),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "Web Shell 应可用: {body}");
    assert!(
        body.get("stdout")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("udx-shell-ok"))
            .unwrap_or(false),
        "应返回真实命令输出: {body}"
    );
    assert_eq!(body.get("exit_code").and_then(|v| v.as_i64()), Some(0));

    // 审计应记录 shell.execute
    let (status, _, logs) = call(
        &app,
        req(Method::GET, "/api/audit/logs", None, Some(&token)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        logs.to_string().contains("shell.execute"),
        "Web Shell 调用必须被审计: {logs}"
    );
}

#[tokio::test]
async fn all_device_write_endpoints_require_token() {
    let (app, _) = test_app().await;
    let paths = [
        "/api/at",
        "/api/set_network",
        "/api/switch",
        "/api/airplane_mode",
        "/api/lock_bands",
        "/api/unlock_bands",
        "/api/lock_cell",
        "/api/usb-advance",
        "/api/sms/send",
        "/api/update/install",
        "/api/plugins",
        "/api/scripts",
        "/api/clear_cache",
        "/api/device_control",
        "/api/shell",
    ];
    for path in paths {
        let (status, _, body) = call(
            &app,
            req(Method::POST, path, Some(json!({})), None),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path} 未认证却被放行: {body}");
    }
}

#[tokio::test]
async fn login_resists_sql_injection_and_brute_force() {
    let (app, _) = test_app().await;
    let payload = json!({"password": "admin' OR '1'='1' --"});
    let (status, _, _) = call(
        &app,
        req(Method::POST, "/api/auth/login", Some(payload), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "注入载荷不得登录成功");

    for _ in 0..5 {
        let _ = call(
            &app,
            req(
                Method::POST,
                "/api/auth/login",
                Some(json!({"password": "wrong"})),
                None,
            ),
        )
        .await;
    }
    let (status, _, _) = call(
        &app,
        req(
            Method::POST,
            "/api/auth/login",
            Some(json!({"password": "admin"})),
            None,
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "登录失败应触发限速");
}

#[tokio::test]
async fn legacy_vue_plugin_templates_are_rejected() {
    let (app, _) = test_app().await;
    let token = login(&app).await;
    let (status, _, body) = call(
        &app,
        req(
            Method::POST,
            "/api/plugins",
            Some(json!({"name": "legacy", "template": "<div>{{ evil }}</div>", "js": "alert(1)"})),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "旧版任意 JS 模板必须拒绝: {body}");
}

#[tokio::test]
async fn static_index_is_embedded_with_strict_csp() {
    let (app, _) = test_app().await;
    let (status, headers, body) = call(&app, req(Method::GET, "/", None, None)).await;
    assert_eq!(status, StatusCode::OK, "入口页应可用: {body}");
    let csp = headers
        .get(header::CONTENT_SECURITY_POLICY)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(csp.contains("default-src 'self'"), "CSP 缺失: {csp}");
    assert!(!csp.contains("unsafe-eval"), "CSP 不得包含 unsafe-eval: {csp}");
}

#[tokio::test]
async fn plugin_data_is_isolated_between_plugins() {
    let (app, _) = test_app().await;
    let token = login(&app).await;
    // 安装一个声明了存储权限的插件，再写入数据；另一个插件读不到。
    let manifest = json!({"id": "demo.status", "name": "Demo", "version": "1.0.0", "min_api_version": 1});
    let ui = json!({
        "schema_version": 1,
        "id": "demo.status",
        "title": "Demo",
        "permissions": ["plugin.storage.write"],
        "view": {"type": "column", "children": [{"type": "text", "bind": "device.version"}]}
    });
    let permissions = json!({"schema_version": 1, "permissions": ["plugin.storage.write"], "max_storage_bytes": 65536});
    let (status, _, body) = call(
        &app,
        req(
            Method::POST,
            "/api/plugins",
            Some(json!({"manifest": manifest, "ui": ui, "permissions": permissions})),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "合规插件应可安装: {body}");

    let (status, _, body) = call(
        &app,
        req(
            Method::POST,
            "/api/plugins/storage/demo.status",
            Some(json!({"k": "v"})),
            Some(&token),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "插件数据写入应成功: {body}");

    let (_, _, body) = call(
        &app,
        req(Method::GET, "/api/plugins/storage/other.plugin", None, Some(&token)),
    )
    .await;
    assert_eq!(body.get("k").and_then(|v| v.as_str()), None);
}

/// 浏览器 `WebSocket` 无法设置 `Authorization` 头：仅在带 `Upgrade: websocket`
/// 的实时通道握手上接受 `?token=`；该回退不得泛化到普通 REST 请求。
#[tokio::test]
async fn websocket_query_token_is_accepted_only_on_upgrade() {
    let (app, _) = test_app().await;
    let token = login(&app).await;

    fn ws_request(path: &str, query: &str, upgrade: bool) -> Request<Body> {
        let mut b = Request::builder()
            .method(Method::GET)
            .uri(format!("{path}{query}"))
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==");
        if upgrade {
            b = b
                .header(header::UPGRADE, "websocket")
                .header(header::CONNECTION, "Upgrade");
        }
        b.body(Body::empty()).unwrap()
    }

    async fn status_only(app: &Router, request: Request<Body>) -> StatusCode {
        // 只读状态码、不消费 body：oneshot 不提供 hyper 的 OnUpgrade 扩展，
        // 握手最终可能落在 4xx，但只要不是 401/403 就证明认证闸门已放行。
        app.clone().oneshot(request).await.expect("router 应可调用").status()
    }

    // 1. 无 token 的 WS 握手一律拒绝
    for path in ["/api/ws", "/api/terminal/ws"] {
        let s = status_only(&app, ws_request(path, "", true)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{path} 无 token 应被拒绝");
    }

    // 2. 合法升级握手 + query token：闸门放行（含 Web 终端的 terminal.session 权限）
    for path in ["/api/ws", "/api/terminal/ws"] {
        let s = status_only(&app, ws_request(path, &format!("?token={token}"), true)).await;
        assert!(
            s != StatusCode::UNAUTHORIZED && s != StatusCode::FORBIDDEN,
            "{path} 的 WS query token 应通过闸门，实际 {s}"
        );
    }

    // 3. 普通 GET（无 Upgrade 头）即便带 query token 也不放行
    let s = status_only(&app, ws_request("/api/terminal/ws", &format!("?token={token}"), false)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "普通 GET 不得读取 query token");

    // 4. 非实时通道路径不接受 query token（尤其高风险的 /api/shell）
    let s = status_only(&app, ws_request("/api/shell", &format!("?token={token}"), true)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "/api/shell 不得接受 query token");
}
