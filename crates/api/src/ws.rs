//! WebSocket / SSE。
//!
//! - `/api/terminal/ws`：**用户要求保留的交互式 Web 任意 Shell**；
//!   会话有总时长上限、累计输出上限、进程组清理与全量审计；
//! - `/api/ws`：只读健康状态推送；
//! - `/api/events`：SSE 降级通道，只推送非敏感健康数据。
//!
//! 认证：浏览器 WebSocket 无法携带 `Authorization` 头，握手时由 `gate` 从
//! `?token=` 校验并把 `SessionToken` 放入请求扩展；`terminal_session` 在**每条
//! input 帧**上重新 `auth.verify`，Token 撤销 / 过期 / 权限回收后立即可终止会话，
//! 不依赖会话超时兜底。

use crate::middleware::SessionToken;
use crate::response::{ApiError, ApiResult};
use crate::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Extension,
};
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use udx_domain::perms;
use udx_domain::state::ActionOutcome;
use udx_domain::Principal;
use udx_platform_linux::{redact_sensitive, InteractiveShell, ShellReader, ShellWriter};
use udx_services::AppServices;

/// 交互式 Web Shell 升级入口。权限检查在 `ShellExecutor::spawn_interactive` 内完成。
pub async fn terminal_ws(
    ws: WebSocketUpgrade,
    Extension(principal): Extension<Principal>,
    Extension(session): Extension<SessionToken>,
    State(state): State<AppState>,
) -> ApiResult<Response> {
    let mut shell = state
        .services
        .shell
        .spawn_interactive(&principal)
        .map_err(ApiError::from)?;
    let (stdin, stdout) = shell.take_streams().map_err(ApiError::from)?;
    let services = state.services.clone();
    Ok(ws.on_upgrade(move |socket| {
        terminal_session(socket, shell, stdin, stdout, services, principal, session.0)
    }))
}

async fn terminal_session(
    mut socket: WebSocket,
    shell: InteractiveShell,
    mut stdin: ShellWriter,
    mut stdout: ShellReader,
    services: Arc<AppServices>,
    principal: Principal,
    raw_token: String,
) {
    let policy = shell.policy.clone();
    let started = Instant::now();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

    let tx_out = out_tx.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if tx_out.send(text).is_err() {
                        break;
                    }
                }
            }
        }
    });

    // PTY 会把 stderr 合并进主端，因此只需要读一路输出。
    drop(out_tx);

    let mut total_output = 0usize;
    let mut outcome = ActionOutcome::Success;
    let limit = tokio::time::sleep(policy.session_timeout);
    tokio::pin!(limit);

    loop {
        tokio::select! {
            _ = &mut limit => {
                outcome = ActionOutcome::Timeout;
                break;
            }
            frame = socket.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        let msg: Value = serde_json::from_str(&text).unwrap_or(json!({}));
                        match msg.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                            "input" => {
                                // 逐帧复核：Token 被撤销/过期或权限回收后，已建立的
                                // 长连接立即终止，而不是等到会话总时长耗尽。
                                let still_valid = match services.auth.verify(&raw_token).await {
                                    Ok(p) => p.can(perms::TERMINAL_SESSION),
                                    Err(_) => false,
                                };
                                if !still_valid {
                                    outcome = ActionOutcome::Failed;
                                    services.shell.record(
                                        &principal,
                                        "terminal.session.revoked",
                                        perms::TERMINAL_SESSION,
                                        ActionOutcome::Failed,
                                        started.elapsed().as_millis() as u64,
                                    );
                                    let _ = socket.send(Message::Text(
                                        json!({"type":"error","message":"会话凭证已失效，连接终止"})
                                            .to_string(),
                                    )).await;
                                    break;
                                }
                                let data = msg.get("data").and_then(|v| v.as_str()).unwrap_or("");
                                // 审计每条输入（凭据脱敏），命令原文不进普通日志。
                                services.shell.record_with_detail(
                                    &principal,
                                    "terminal.session.input",
                                    perms::TERMINAL_SESSION,
                                    ActionOutcome::Success,
                                    0,
                                    redact_sensitive(data),
                                );
                                if stdin.write_all(data.as_bytes()).await.is_err()
                                    || stdin.flush().await.is_err()
                                {
                                    break;
                                }
                            }
                            "resize" => {
                                let cols = msg
                                    .get("cols")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(120)
                                    .clamp(1, 1000) as u16;
                                let rows = msg
                                    .get("rows")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(32)
                                    .clamp(1, 500) as u16;
                                // PTY 会话：真正下发 TIOCSWINSZ，kernel 会向 shell
                                // 触发 SIGWINCH，top / vim 等全屏程序才能正确重排。
                                match shell.resize(cols, rows) {
                                    Ok(()) => {
                                        let _ = socket
                                            .send(Message::Text(
                                                json!({"type":"info","message":"终端尺寸已更新","cols":cols,"rows":rows})
                                                    .to_string(),
                                            ))
                                            .await;
                                    }
                                    Err(e) => {
                                        let _ = socket
                                            .send(Message::Text(
                                                json!({"type":"error","message":e.public_message()})
                                                    .to_string(),
                                            ))
                                            .await;
                                    }
                                }
                            }
                            "close" => break,
                            _ => {
                                let _ = socket
                                    .send(Message::Text(
                                        json!({"type":"error","message":"未知消息类型"}).to_string(),
                                    ))
                                    .await;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
            Some(chunk) = out_rx.recv() => {
                total_output += chunk.len();
                if total_output > policy.session_output_cap {
                    outcome = ActionOutcome::Failed;
                    let _ = socket.send(Message::Text(
                        json!({"type":"error","message":"会话输出超过上限，已终止"}).to_string(),
                    )).await;
                    break;
                }
                let payload = json!({"type":"output","data":chunk}).to_string();
                if socket.send(Message::Text(payload)).await.is_err() {
                    break;
                }
            }
        }
    }

    let _ = socket
        .send(Message::Text(
            json!({"type":"exit","code":0,"reason":"session_closed"}).to_string(),
        ))
        .await;
    services.shell.record(
        &principal,
        "terminal.session.end",
        perms::TERMINAL_SESSION,
        outcome,
        started.elapsed().as_millis() as u64,
    );
    // 显式释放：InteractiveShell::drop 会终止整个进程组并归还并发许可。
    drop(shell);
}

/// 只读状态 WebSocket（前端 websocket.js 使用）。
/// `/api/ws`：状态推送。
///
/// 除健康摘要外，同时推送前端 `wsSubscribe('stats'|'traffic'|'signal')` 订阅的
/// 三类实时数据。此前只推 `health`，导致页面的「实时趋势」只能靠 5 秒轮询，
/// 信号曲线永远是 0。
pub async fn status_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(2));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last_total: u64 = 0;
        let mut last_at = Instant::now();
        loop {
            ticker.tick().await;

            let health = json!({
                "type": "health",
                "uptime_secs": state.services.started_at.elapsed().as_secs(),
                "dbus_ok": state.services.dbus_ok.load(Ordering::Relaxed),
                "active_tasks": state.services.tasks.active(),
            });

            // 系统资源：CPU / 内存（/proc 只读，开销极小）。
            let stats = match state
                .services
                .network
                .system_info(env!("CARGO_PKG_VERSION"))
                .await
            {
                Ok(info) => json!({
                    "type": "stats",
                    "data": {
                        "cpu_usage": info.get("cpu_usage").cloned().unwrap_or(json!(0.0)),
                        "mem_used": info.get("mem_used").cloned().unwrap_or(json!(0)),
                        "mem_total": info.get("mem_total").cloned().unwrap_or(json!(0)),
                        "temperature": info.get("temperature").cloned().unwrap_or(Value::Null),
                    }
                }),
                Err(_) => json!({"type": "stats", "data": {"cpu_usage": 0.0}}),
            };

            // 流量速率：相邻两次采样的字节差 / 时间差。总计数回落说明设备重启过。
            let total = state
                .services
                .network
                .traffic_total()
                .await
                .ok()
                .and_then(|v| v.get("total").and_then(|t| t.as_u64()))
                .unwrap_or(last_total);
            let now = Instant::now();
            let elapsed = now.duration_since(last_at).as_secs_f64().max(0.001);
            let rate = if last_total == 0 {
                0.0
            } else {
                total.saturating_sub(last_total) as f64 / elapsed
            };
            last_total = total;
            last_at = now;
            let traffic = json!({
                "type": "traffic",
                "data": {"rate": rate.round() as u64, "total": total}
            });

            // 信号：无 Modem 对象时 rsrp 为 null，前端显示「—」而不是 0 dBm。
            let rsrp = state
                .services
                .modem
                .current_band()
                .await
                .ok()
                .and_then(|v| v.get("rsrp").cloned())
                .unwrap_or(Value::Null);
            let signal = json!({
                "type": "signal",
                "data": {"rsrp": rsrp}
            });

            for payload in [health, stats, traffic, signal] {
                if socket.send(Message::Text(payload.to_string())).await.is_err() {
                    return;
                }
            }
        }
    })
    .into_response()
}

/// SSE 降级通道：只推送非敏感的健康摘要。
pub async fn events(
    State(state): State<AppState>,
) -> Sse<impl stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::unfold(state, |state| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let payload = json!({
            "type": "health",
            "uptime_secs": state.services.started_at.elapsed().as_secs(),
            "dbus_ok": state.services.dbus_ok.load(Ordering::Relaxed),
        });
        Some((
            Ok::<_, Infallible>(Event::default().data(payload.to_string())),
            state,
        ))
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// 供测试调用：确认会话策略仍保留 Web Shell。
#[cfg(test)]
mod tests {
    use udx_platform_linux::ShellPolicy;

    #[test]
    fn web_shell_is_enabled_by_default() {
        assert!(ShellPolicy::default().enabled);
    }
}
