//! Local HTTP IPC server for Phase-5 integrations.
//!
//! Binds to `127.0.0.1:<port>` (loopback only — never listens on 0.0.0.0),
//! accepts two endpoints:
//!
//!   - `POST /event` with JSON body `{ "type": "<name>", "payload": <any> }`.
//!     Emits a Tauri event `hook_event` with that body. The JS behavior
//!     engine's hook_reactions.js picks up the event and animates Pane.
//!   - `GET /health` — returns 200 for liveness checks.
//!
//! Callers: the bundled `claude-companion-event` CLI (for Claude Code hooks)
//! and `claude-companion-mcp` (for MCP tool dispatch). Both ship beside the
//! main app binary.
//!
//! Why HTTP instead of a unix socket or named pipe? HTTP is boring: the CLI
//! is std-only (no client deps), a curl request is trivial to test with,
//! and axum is standard in the Tauri ecosystem.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tauri::{AppHandle, Emitter};

#[derive(Clone)]
struct IpcState {
    app: AppHandle,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HookEvent {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

async fn handle_event(
    State(state): State<IpcState>,
    Json(evt): Json<HookEvent>,
) -> StatusCode {
    // Diagnostic: log every received event. Invaluable when hooks/MCP
    // silently don't produce a visible reaction — lets us confirm whether
    // the event reached us at all, vs. dying upstream.
    log_diag(&format!(
        "event received: type={} payload={}",
        evt.type_,
        serde_json::to_string(&evt.payload).unwrap_or_default(),
    ));
    // Forward to the frontend. If the emit fails (window closed, etc.) we
    // still return OK — the caller doesn't need to know about our internal
    // plumbing, and a failed emit shouldn't break the hook pipeline.
    match state.app.emit("hook_event", &evt) {
        Ok(_) => log_diag("emit OK"),
        Err(e) => log_diag(&format!("emit FAILED: {e}")),
    }
    StatusCode::OK
}

async fn handle_health() -> StatusCode {
    StatusCode::OK
}

/// Spawn the IPC server on the given port.
///
/// The server runs on its own OS thread with a dedicated tokio runtime —
/// NOT the Tauri async runtime. We learned the hard way that tasks spawned
/// on `tauri::async_runtime::spawn` from inside the setup closure could
/// silently fail to start if setup ran before the global runtime was ready.
/// A standalone thread + its own runtime is a couple of KB of overhead and
/// eliminates the timing ambiguity entirely.
pub fn spawn_server(app: AppHandle, port: u16) {
    // Dual-log: eprintln AND write to /tmp. The tauri dev terminal is the
    // natural home for these messages, but in practice it's easy to miss
    // them in the build spam — /tmp gives us a reliable debug trail that
    // survives restarts.
    log_diag("spawn_server called");

    let state = IpcState { app };
    let router = Router::new()
        .route("/event", post(handle_event))
        .route("/health", get(handle_health))
        .with_state(state);

    let spawn_result = std::thread::Builder::new()
        .name("ipc-server".into())
        .spawn(move || {
            log_diag("ipc-server thread started");
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    log_diag(&format!("failed to build tokio runtime: {e}"));
                    eprintln!("[ipc] failed to build tokio runtime: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                let addr = SocketAddr::from(([127, 0, 0, 1], port));
                log_diag(&format!("attempting bind on {addr}"));
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(listener) => {
                        log_diag(&format!("LISTENING on http://{addr}"));
                        eprintln!("[ipc] listening on http://{addr}");
                        if let Err(e) = axum::serve(listener, router).await {
                            log_diag(&format!("server error: {e}"));
                            eprintln!("[ipc] server error: {e}");
                        }
                    }
                    Err(e) => {
                        log_diag(&format!("bind {addr} failed: {e}"));
                        eprintln!("[ipc] bind {addr} failed: {e}");
                    }
                }
            });
        });

    if let Err(e) = spawn_result {
        log_diag(&format!("thread spawn failed: {e}"));
        eprintln!("[ipc] thread spawn failed: {e}");
    }
}

/// Append a timestamped diagnostic line to /tmp/claude-companion-ipc.log.
/// Swallows all errors — diagnostic logging should never break the app.
fn log_diag(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/claude-companion-ipc.log")
    {
        let _ = writeln!(
            f,
            "[{}] {}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            msg
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Validates the JSON wire format the CLI and MCP binary both emit.
    #[test]
    fn hook_event_deserializes_minimal() {
        let raw = r#"{"type":"Stop","payload":null}"#;
        let parsed: HookEvent = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.type_, "Stop");
        assert!(parsed.payload.is_null());
    }

    #[test]
    fn hook_event_deserializes_with_object_payload() {
        let raw = r#"{"type":"PreToolUse","payload":{"tool":"Bash","cmd":"ls"}}"#;
        let parsed: HookEvent = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.type_, "PreToolUse");
        assert_eq!(parsed.payload["tool"], "Bash");
    }

    #[test]
    fn hook_event_round_trips_via_serde() {
        let evt = HookEvent {
            type_: "mcp".into(),
            payload: serde_json::json!({ "tool": "companion_say", "arguments": { "text": "hi" } }),
        };
        let s = serde_json::to_string(&evt).unwrap();
        let back: HookEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(back.type_, "mcp");
        assert_eq!(back.payload["tool"], "companion_say");
    }
}
