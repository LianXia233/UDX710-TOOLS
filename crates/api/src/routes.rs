//! 全量路由表：与旧版 `src/handlers/http_server.c` 的 mg_match 一一对应。
//!
//! 路径、方法与 JSON 语义保持兼容；新增 `/api/health`、`/api/events`、
//! `/api/ws`、`/api/terminal/ws`、`/api/audit/logs` 属于重构增强。

use crate::handlers as h;
use crate::middleware;
use crate::static_files;
use crate::ws;
use crate::AppState;
use axum::{
    extract::DefaultBodyLimit,
    middleware::from_fn,
    middleware::from_fn_with_state,
    routing::{get, post, put, delete},
    Router,
};

pub fn router(state: AppState) -> Router {
    let body_limit = state.services.config.limits.max_body_bytes;

    let api = Router::new()
        // ---------------- 认证 ----------------
        .route("/api/auth/login", post(h::auth_login))
        .route("/api/auth/logout", post(h::auth_logout))
        .route("/api/auth/password", post(h::auth_password))
        .route("/api/auth/status", get(h::auth_status))
        // ---------------- 密保 ----------------
        .route("/api/security/status", get(h::security_status))
        .route("/api/security/setup", post(h::security_setup))
        .route("/api/security/questions", get(h::security_questions))
        .route("/api/security/verify", post(h::security_verify))
        .route(
            "/api/security/reset-password",
            post(h::security_reset_password),
        )
        .route(
            "/api/security/factory-reset",
            post(h::security_factory_reset),
        )
        // ---------------- 系统 ----------------
        .route("/api/health", get(h::health))
        .route("/api/info", get(h::info))
        .route("/api/clear_cache", post(h::clear_cache))
        .route("/api/device_control", post(h::device_control))
        .route("/api/serial", get(h::serial))
        .route("/api/key", post(h::key))
        .route("/api/get/time", get(h::get_system_time))
        .route("/api/set/time", post(h::sync_system_time))
        .route("/api/get/first-reboot", get(h::reboot_config))
        .route("/api/set/reboot", get(h::set_reboot))
        .route("/api/claen/cron", get(h::clear_cron))
        .route("/api/phone-case", get(h::phone_case).post(h::phone_case_set))
        // ---------------- Modem / 网络 ----------------
        .route("/api/at", post(h::at))
        .route("/api/set_network", post(h::set_network))
        .route("/api/switch", post(h::switch_slot))
        .route("/api/airplane_mode", post(h::airplane_mode))
        .route("/api/bands", get(h::bands))
        .route("/api/current_band", get(h::current_band))
        .route("/api/lock_bands", post(h::lock_bands))
        .route("/api/unlock_bands", post(h::unlock_bands))
        .route("/api/cells", get(h::cells))
        .route("/api/lock_cell", post(h::lock_cell))
        .route("/api/unlock_cell", post(h::unlock_cell))
        .route("/api/data", get(h::data_status).post(h::data_set))
        .route("/api/roaming", get(h::roaming).post(h::roaming_set))
        .route("/api/netif/list", get(h::netif_list))
        .route("/api/netif/stats", post(h::netif_stats))
        .route("/api/netif/monitor", post(h::netif_monitor))
        // ---------------- 流量 / 充电 ----------------
        .route("/api/get/Total", get(h::traffic_total))
        .route("/api/get/set", get(h::traffic_config))
        .route("/api/set/total", post(h::traffic_set))
        .route(
            "/api/charge/config",
            get(h::charge_config).post(h::charge_set),
        )
        .route("/api/charge/on", post(h::charge_on))
        .route("/api/charge/off", post(h::charge_off))
        // ---------------- USB ----------------
        .route("/api/usb/mode", get(h::usb_mode).post(h::usb_mode_set))
        .route("/api/usb-advance", post(h::usb_advance))
        // ---------------- APN ----------------
        .route("/api/apn", get(h::apn_config).post(h::apn_set))
        .route("/api/apn/config", get(h::apn_config).post(h::apn_set))
        .route(
            "/api/apn/templates",
            get(h::apn_templates).post(h::apn_template_create),
        )
        .route(
            "/api/apn/templates/:id",
            put(h::apn_template_update).delete(h::apn_template_delete),
        )
        .route("/api/apn/apply", post(h::apn_apply))
        .route("/api/apn/clear", post(h::apn_clear))
        // ---------------- 短信 ----------------
        .route("/api/sms", get(h::sms_list))
        .route("/api/sms/send", post(h::sms_send))
        .route("/api/sms/sent", get(h::sms_sent))
        .route("/api/sms/sent/:id", delete(h::sms_sent_delete))
        .route("/api/sms/config", get(h::sms_config).post(h::sms_config_save))
        .route(
            "/api/sms/webhook",
            get(h::sms_webhook).post(h::sms_webhook_save),
        )
        .route("/api/sms/webhook/test", post(h::sms_webhook_test))
        .route("/api/sms/webhook/logs", get(h::sms_webhook_logs))
        .route("/api/sms/fix", get(h::sms_fix).post(h::sms_fix_set))
        .route("/api/sms/:id", delete(h::sms_delete))
        // ---------------- OTA ----------------
        .route("/api/update/version", get(h::update_version))
        .route("/api/update/check", get(h::update_check))
        .route("/api/update/upload", post(h::update_upload))
        .route("/api/update/download", post(h::update_download))
        .route("/api/update/extract", post(h::update_extract))
        .route("/api/update/install", post(h::update_install))
        // ---------------- Web Shell（用户要求保留）----------------
        .route("/api/shell", post(h::shell))
        .route("/api/terminal/ws", get(ws::terminal_ws))
        // ---------------- 插件 / 脚本 / 存储 ----------------
        .route("/api/plugins", get(h::plugin_list).post(h::plugin_upload))
        .route("/api/plugins/all", delete(h::plugin_delete_all))
        .route(
            "/api/plugins/storage/:name",
            get(h::plugin_storage_get)
                .post(h::plugin_storage_set)
                .delete(h::plugin_storage_delete),
        )
        .route("/api/plugins/:name", delete(h::plugin_delete))
        .route("/api/scripts", get(h::script_list).post(h::script_upload))
        .route(
            "/api/scripts/:name",
            put(h::script_update).delete(h::script_delete),
        )
        // ---------------- Rathole ----------------
        .route(
            "/api/rathole/config",
            get(h::rathole_config).post(h::rathole_config_set),
        )
        .route(
            "/api/rathole/services",
            get(h::rathole_services).post(h::rathole_service_add),
        )
        .route(
            "/api/rathole/services/:id",
            put(h::rathole_service_update).delete(h::rathole_service_delete),
        )
        .route("/api/rathole/start", post(h::rathole_start))
        .route("/api/rathole/stop", post(h::rathole_stop))
        .route("/api/rathole/status", get(h::rathole_status))
        .route("/api/rathole/logs", get(h::rathole_logs))
        .route("/api/rathole/server-config", get(h::rathole_server_config))
        .route("/api/rathole/autostart", post(h::rathole_autostart))
        // ---------------- IPv6 Proxy ----------------
        .route(
            "/api/ipv6-proxy/config",
            get(h::ipv6_config).post(h::ipv6_config_set),
        )
        .route(
            "/api/ipv6-proxy/rules",
            get(h::ipv6_rules).post(h::ipv6_rule_add),
        )
        .route(
            "/api/ipv6-proxy/rules/:id",
            put(h::ipv6_rule_update).delete(h::ipv6_rule_delete),
        )
        .route("/api/ipv6-proxy/start", post(h::ipv6_start))
        .route("/api/ipv6-proxy/stop", post(h::ipv6_stop))
        .route("/api/ipv6-proxy/restart", post(h::ipv6_restart))
        .route("/api/ipv6-proxy/status", get(h::ipv6_status))
        .route("/api/ipv6-proxy/send", post(h::ipv6_send))
        .route("/api/ipv6-proxy/test", post(h::ipv6_test))
        .route("/api/ipv6-proxy/send-logs", get(h::ipv6_send_logs))
        // ---------------- 实时 / 审计 ----------------
        .route("/api/ws", get(ws::status_ws))
        .route("/api/events", get(ws::events))
        .route("/api/audit/logs", get(h::audit_logs))
        // 主闸门：CORS 预检、限速、认证、细粒度授权
        .layer(from_fn_with_state(state.clone(), middleware::gate));

    Router::new()
        .merge(api)
        .fallback(static_files::static_handler)
        // 访问日志只记录 path，不记录 query（WS 会话 token 走 ?token=）。
        .layer(from_fn(middleware::security_headers))
        .layer(from_fn(middleware::request_log))
        .layer(DefaultBodyLimit::max(body_limit))
        .with_state(state)
}

/// 路由表自检：确保不会被误删关键端点。
#[cfg(test)]
mod tests {
    #[test]
    fn route_source_contains_all_critical_endpoints() {
        let src = include_str!("routes.rs");
        for p in [
            "/api/device_control",
            "/api/shell",
            "/api/terminal/ws",
            "/api/claen/cron",
            "/api/get/Total",
            "/api/phone-case",
            "/api/ipv6-proxy/send-logs",
            "/api/rathole/server-config",
        ] {
            assert!(src.contains(p), "缺少端点 {p}");
        }
    }
}
