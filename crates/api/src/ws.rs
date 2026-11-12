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
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use udx_domain::perms;
use udx_domain::state::ActionOutcome;
use udx_domain::Principal;
use udx_platform_linux::{redact_sensitive, InteractiveShell};
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
    let (stdin, stdout, stderr) = shell.take_streams().map_err(ApiError::from)?;
    let services = state.services.clone();
    Ok(ws.on_upgrade(move |socket| {
        terminal_session(
            socket,
            shell,
            stdin,
            stdout,
            stderr,
            services,
            principal,
            session.0,
        )
    }))
}

#[allow(clippy::too_many_arguments)]
async fn terminal_session(
    mut socket: WebSocket,
    shell: InteractiveShell,
    mut stdin: ChildStdin,
    mut stdout: ChildStdout,
    mut stderr: ChildStderr,
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

    let tx_err = out_tx.clone();
    tokio::spawn(async move {
        let mut buf = [0u8; 4096];
        loop {
            match stderr.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if tx_err.send(text).is_err() {
                        break;
                    }
                }
            }
        }
    });
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
                                // 非 PTY 会话，忽略尺寸变化但确认客户端。
                                let _ = socket
                                    .send(Message::Text(
                                        json!({"type":"info","message":"PTY 未启用，尺寸变化被忽略"})
                                            .to_string(),
                                    ))
                                    .await;
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
pub async fn status_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(move |mut socket| async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(5));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let payload = json!({
                "type": "health",
                "uptime_secs": state.services.started_at.elapsed().as_secs(),
                "dbus_ok": state.services.dbus_ok.load(Ordering::Relaxed),
                "active_tasks": state.services.tasks.active(),
            })
            .to_string();
            if socket.send(Message::Text(payload)).await.is_err() {
                break;
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
