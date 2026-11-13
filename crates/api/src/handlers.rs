//! HTTP Handler：只调用 Service，不拼 Shell、不碰 sysfs、不写 SQL。
//!
//! 所有路径、HTTP 方法与 JSON 字段与旧版保持兼容（见 docs/API_CONTRACT.md）。
//! 少数新增字段（如 `device_verified`、`outcome`）用于表达明确的设备动作状态，
//! 不覆盖旧字段。

use crate::response::{wrap, ApiError, ApiResult};
use crate::AppState;
use axum::{
    extract::{ConnectInfo, Path, Query, State},
    Extension, Json,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::future::Future;
use std::net::SocketAddr;
use std::time::Instant;
use udx_domain::error::{AppError, AppResult};
use udx_domain::perms;
use udx_domain::state::ActionOutcome;
use udx_domain::Principal;

const VERSION: &str = env!("CARGO_PKG_VERSION");

async fn done(fut: impl Future<Output = AppResult<Value>>) -> ApiResult<Json<Value>> {
    wrap(fut.await)
}

fn field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("")
}

fn outcome_of(v: &Value) -> ActionOutcome {
    match v.get("outcome").and_then(|x| x.as_str()).unwrap_or("unknown") {
        "success" => ActionOutcome::Success,
        "failed" | "error" => ActionOutcome::Failed,
        "timeout" => ActionOutcome::Timeout,
        _ => ActionOutcome::Unknown,
    }
}

fn to_json<T: serde::Serialize>(value: T) -> ApiResult<Json<Value>> {
    serde_json::to_value(value)
        .map(Json)
        .map_err(|e| ApiError::from(AppError::Internal(format!("序列化失败: {e}"))))
}

// ============================ 认证 ============================

pub async fn auth_login(
    State(s): State<AppState>,
    client: Option<ConnectInfo<SocketAddr>>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    let key = client
        .map(|c| c.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let token = s
        .services
        .auth
        .login(field(&body, "password"), &key)
        .await?;
    Ok(Json(json!({"status": "ok", "token": token})))
}

pub async fn auth_logout(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
) -> ApiResult<Json<Value>> {
    s.services.auth.logout(&p).await?;
    Ok(Json(json!({"status": "ok"})))
}

pub async fn auth_password(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    s.services
        .auth
        .change_password(
            &p,
            field(&body, "old_password"),
            field(&body, "new_password"),
        )
        .await?;
    Ok(Json(json!({"status": "ok"})))
}

pub async fn auth_status(
    State(s): State<AppState>,
    principal: Option<Extension<Principal>>,
) -> ApiResult<Json<Value>> {
    done(s.services.auth.status(principal.as_ref().map(|e| &e.0))).await
}

// ============================ 系统 ============================

pub async fn info(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.system_info(VERSION)).await
}

pub async fn health(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    // 保留旧版全部字段，额外暴露看门狗状态供前端 / supervisor 观测。
    let mut value = serde_json::to_value(s.services.health().await?)
        .map_err(|e| ApiError::from(AppError::Internal(format!("序列化失败: {e}"))))?;
    let watchdog = serde_json::to_value(s.services.watchdog.status())
        .map_err(|e| ApiError::from(AppError::Internal(format!("序列化失败: {e}"))))?;
    value["watchdog"] = watchdog;
    Ok(Json(value))
}

pub async fn clear_cache(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.clear_cache()).await
}

pub async fn device_control(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    let action = field(&b, "action").to_string();
    let started = Instant::now();
    let result = s.services.modem.device_control(&action).await?;
    s.services.record(
        &p,
        "device_control",
        perms::DEVICE_CONTROL,
        outcome_of(&result),
        action.clone(),
        started.elapsed().as_millis() as u64,
    );
    Ok(Json(result))
}

pub async fn serial(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.serial()).await
}

pub async fn key(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.submit_key(field(&b, "key"))).await
}

pub async fn get_system_time(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.get_system_time()).await
}

pub async fn set_system_time(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.set_system_time(&b)).await
}

pub async fn sync_system_time(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.sync_system_time()).await
}

pub async fn reboot_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.reboot_config()).await
}

pub async fn set_reboot(
    State(s): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Value>> {
    let b = json!({
        "day": q.get("day").cloned().unwrap_or_default(),
        "hour": q.get("hour").and_then(|x| x.parse::<u64>().ok()).unwrap_or(0),
        "minute": q.get("minute").and_then(|x| x.parse::<u64>().ok()).unwrap_or(0),
    });
    done(s.services.network.reboot_set(&b)).await
}

pub async fn clear_cron(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.reboot_clear()).await
}

pub async fn phone_case(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.phone_case_get()).await
}

pub async fn phone_case_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.network.phone_case_set(&b)).await
}

// ============================ 网络 / Modem ============================

pub async fn at(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.execute_at(field(&b, "command"))).await
}

pub async fn set_network(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.set_network(&b)).await
}

pub async fn switch_slot(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.switch_slot(&b)).await
}

pub async fn airplane_mode(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.airplane_mode(&b)).await
}

pub async fn bands(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.bands()).await
}

pub async fn current_band(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.current_band()).await
}

pub async fn lock_bands(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.lock_bands(&b)).await
}

pub async fn unlock_bands(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.unlock_bands()).await
}

pub async fn cells(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.cells()).await
}

pub async fn lock_cell(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.lock_cell(&b)).await
}

pub async fn unlock_cell(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.unlock_cell()).await
}

pub async fn data_status(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.data_status()).await
}

pub async fn data_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.set_data(&b)).await
}

pub async fn roaming(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.modem.roaming()).await
}

pub async fn roaming_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.modem.set_roaming(&b)).await
}

pub async fn netif_list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.netif_list()).await
}

pub async fn netif_stats(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.network.netif_stats(&b)).await
}

pub async fn netif_monitor(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.network.netif_monitor(&b)).await
}

// ============================ 流量 / 充电 ============================

pub async fn traffic_total(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.traffic_total()).await
}

pub async fn traffic_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.traffic_config()).await
}

pub async fn traffic_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.network.traffic_set(&b)).await
}

pub async fn charge_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.charge_config()).await
}

pub async fn charge_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.network.charge_set(&b)).await
}

pub async fn charge_on(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.charge_on()).await
}

pub async fn charge_off(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.charge_off()).await
}

// ============================ USB ============================

pub async fn usb_mode(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.usb.get_mode()).await
}

pub async fn usb_mode_set(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    let started = Instant::now();
    let result = s.services.usb.set_mode(&b).await?;
    s.services.record(
        &p,
        "usb.switch",
        perms::USB_SWITCH,
        outcome_of(&result),
        field(&b, "mode").to_string(),
        started.elapsed().as_millis() as u64,
    );
    Ok(Json(result))
}

pub async fn usb_advance(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    let started = Instant::now();
    let result = s.services.usb.advance(&b).await?;
    s.services.record(
        &p,
        "usb.advance",
        perms::USB_SWITCH,
        outcome_of(&result),
        field(&b, "mode").to_string(),
        started.elapsed().as_millis() as u64,
    );
    Ok(Json(result))
}

// ============================ APN ============================

pub async fn apn_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_config()).await
}

pub async fn apn_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_set(&b)).await
}

pub async fn apn_templates(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_templates_list()).await
}

pub async fn apn_template_create(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_template_create(&b)).await
}

pub async fn apn_template_update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_template_update(&id, &b)).await
}

pub async fn apn_template_delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_template_delete(&id)).await
}

pub async fn apn_apply(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_apply()).await
}

pub async fn apn_clear(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.network.apn_clear()).await
}

// ============================ 短信 ============================

pub async fn sms_list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.sms.list()).await
}

pub async fn sms_send(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.sms.send(&b)).await
}

pub async fn sms_delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.sms.delete(&id)).await
}

pub async fn sms_sent(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.sms.sent_list()).await
}

pub async fn sms_sent_delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.sms.sent_delete(&id)).await
}

pub async fn sms_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.sms.config_get()).await
}

pub async fn sms_config_save(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.sms.config_save(&b)).await
}

pub async fn sms_webhook(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.sms.webhook_get()).await
}

pub async fn sms_webhook_save(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.sms.webhook_save(&b)).await
}

pub async fn sms_webhook_test(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.sms.webhook_test(Some(&b))).await
}

pub async fn sms_webhook_logs(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.sms.webhook_logs()).await
}

pub async fn sms_fix(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.sms.fix_get()).await
}

pub async fn sms_fix_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.sms.fix_set(&b)).await
}

// ============================ 更新 ============================

pub async fn update_version(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.ota.version()).await
}

pub async fn update_check(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.ota.check()).await
}

pub async fn update_upload(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.ota.upload(&b)).await
}

pub async fn update_download(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.ota.download(&b)).await
}

pub async fn update_extract(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.ota.extract()).await
}

pub async fn update_install(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    let started = Instant::now();
    let result = s.services.ota.install(&b).await?;
    s.services.record(
        &p,
        "ota.install",
        perms::OTA_INSTALL,
        outcome_of(&result),
        "install".to_string(),
        started.elapsed().as_millis() as u64,
    );
    Ok(Json(result))
}

// ============================ 插件 / 脚本 / Web Shell ============================

/// **用户明确要求保留的 Web 任意 Shell**（一次性执行）。
pub async fn shell(
    State(s): State<AppState>,
    Extension(p): Extension<Principal>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    let result = s.services.shell.execute(&p, field(&b, "command")).await?;
    to_json(result)
}

pub async fn plugin_list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.list()).await
}

pub async fn plugin_upload(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.install(&b)).await
}

pub async fn plugin_delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.delete(&id)).await
}

pub async fn plugin_delete_all(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.delete_all()).await
}

pub async fn plugin_storage_get(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.storage_get(&id)).await
}

pub async fn plugin_storage_set(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    done(s.services.plugin.storage_set(&id, &b)).await
}

pub async fn plugin_storage_delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.storage_delete(&id)).await
}

pub async fn script_list(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.script_list()).await
}

pub async fn script_upload(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.script_upload(&b)).await
}

pub async fn script_update(
    State(s): State<AppState>,
    Path(name): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    done(s.services.plugin.script_update(&name, &b)).await
}

pub async fn script_delete(State(s): State<AppState>, Path(name): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.plugin.script_delete(&name)).await
}

// ============================ Rathole ============================

pub async fn rathole_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_config()).await
}

pub async fn rathole_config_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_config_set(&b)).await
}

pub async fn rathole_services(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_services()).await
}

pub async fn rathole_service_add(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_service_add(&b)).await
}

pub async fn rathole_service_update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_service_update(&id, &b)).await
}

pub async fn rathole_service_delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_service_delete(&id)).await
}

macro_rules! proxy_action {
    ($fn:ident, $svc:ident, $action:expr) => {
        pub async fn $fn(State(s): State<AppState>) -> ApiResult<Json<Value>> {
            done(s.services.proxy.$svc($action)).await
        }
    };
}

proxy_action!(rathole_start, rathole_action, "start");
proxy_action!(rathole_stop, rathole_action, "stop");

pub async fn rathole_status(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_status()).await
}

pub async fn rathole_logs(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_logs()).await
}

pub async fn rathole_server_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_server_config()).await
}

pub async fn rathole_autostart(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.rathole_autostart(&b)).await
}

// ============================ IPv6 Proxy ============================

pub async fn ipv6_config(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_config()).await
}

pub async fn ipv6_config_set(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_config_set(&b)).await
}

pub async fn ipv6_rules(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_rules()).await
}

pub async fn ipv6_rule_add(State(s): State<AppState>, Json(b): Json<Value>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_rule_add(&b)).await
}

pub async fn ipv6_rule_update(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<Value>,
) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_rule_update(&id, &b)).await
}

pub async fn ipv6_rule_delete(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_rule_delete(&id)).await
}

proxy_action!(ipv6_start, ipv6_action, "start");
proxy_action!(ipv6_stop, ipv6_action, "stop");
proxy_action!(ipv6_restart, ipv6_action, "restart");
proxy_action!(ipv6_send, ipv6_action, "send");
proxy_action!(ipv6_test, ipv6_action, "test");

pub async fn ipv6_status(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_status()).await
}

pub async fn ipv6_send_logs(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    done(s.services.proxy.ipv6_send_logs()).await
}

// ============================ 审计 ============================

/// 审计日志只读接口。敏感字段（密码 / Token / 短信正文）从不落入该表。
pub async fn audit_logs(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let rows = s.services.db.audit_list(200).await?;
    Ok(Json(json!({"logs": rows})))
}
