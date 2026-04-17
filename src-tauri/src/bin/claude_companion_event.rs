//! Claude Code hook bridge — tiny CLI that POSTs a hook event to the
//! companion's local IPC server.
//!
//! Installed into `~/.claude/bin/` and referenced from the user's Claude Code
//! `settings.json` hooks config. Claude Code runs this binary with the hook
//! payload on stdin; we forward it as an HTTP POST to the companion. The
//! companion emits a Tauri event, the JS side picks an animation, Pane reacts.
//!
//! Kept std-only (no reqwest / ureq) so the binary stays tiny — it's shipped
//! inside the Tauri app bundle and has to be fast to cold-start on every
//! single tool-use event.
//!
//! Usage:
//!   claude-companion-event --event PreToolUse              (payload on stdin)
//!   claude-companion-event --event Stop --port 48372

use std::env;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut event_type: Option<String> = None;
    let mut port: u16 = 48372;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--event" | "-e" => {
                i += 1;
                if i < args.len() { event_type = Some(args[i].clone()); }
            }
            "--port" | "-p" => {
                i += 1;
                if i < args.len() { port = args[i].parse().unwrap_or(48372); }
            }
            _ => {}
        }
        i += 1;
    }

    let Some(event_type) = event_type else {
        eprintln!("claude-companion-event: --event <type> required");
        std::process::exit(2);
    };

    // Read payload from stdin if present (Claude Code passes JSON).
    let mut payload = String::new();
    let _ = std::io::stdin().read_to_string(&mut payload);

    // Build the JSON body by hand — no serde dep in this binary; every byte
    // saved here matters for cold-start latency on every tool-use event.
    let payload_trimmed = payload.trim();
    let payload_json: &str = if payload_trimmed.is_empty() { "null" } else { payload_trimmed };
    let body = format!(
        "{{\"type\":{},\"payload\":{}}}",
        json_string(&event_type),
        payload_json,
    );

    let addr = format!("127.0.0.1:{}", port);
    let Ok(mut stream) = TcpStream::connect_timeout(
        &addr.parse().expect("static addr"),
        Duration::from_millis(500),
    ) else {
        // Companion isn't running — silent exit is correct. We don't want to
        // break the user's hook pipeline just because the companion is off.
        std::process::exit(0);
    };

    let request = format!(
        "POST /event HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\r\n{body}",
        port = port,
        len = body.len(),
        body = body,
    );
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    let _ = stream.write_all(request.as_bytes());
    // Intentionally don't wait for a response — fire-and-forget keeps hook
    // latency to single-digit ms.
}

/// Minimal JSON string escaper. Covers the cases hook type names actually
/// hit (letters, digits — no surprises), but we still quote-escape to be
/// safe if Claude Code ever passes something with special chars.
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
