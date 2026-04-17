//! Companion MCP server — stdio JSON-RPC.
//!
//! Spawned by Claude Code after the user adds an entry to their MCP config.
//! Exposes three tools Claude can call:
//!
//!   - `companion_say`:         show a speech bubble on Pane
//!   - `companion_react`:       trigger an emotion-tagged animation
//!   - `companion_show_status`: (reserved; no-op for v1)
//!
//! Each tool call here is forwarded as an HTTP POST to the companion's local
//! IPC server (same endpoint the hook bridge CLI uses). That keeps the MCP
//! binary thin and makes the companion the single point of truth for state.
//!
//! Implements the bare minimum of MCP 2024-11-05 for Claude Code: `initialize`,
//! `tools/list`, `tools/call`. Not a full MCP SDK.

use std::io::{BufRead, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const PORT: u16 = 48372;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { continue; };
        let trimmed = line.trim();
        if trimmed.is_empty() { continue; }

        // Parse just enough of the JSON-RPC frame to route it. Full MCP has
        // notifications (no id), requests (with id), responses — we only
        // need to answer requests Claude Code sends us.
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(trimmed) else { continue; };
        let method = msg.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let id = msg.get("id").cloned();
        if id.is_none() { continue; } // notification, no response needed

        let result = match method {
            "initialize" => handle_initialize(),
            "tools/list" => handle_tools_list(),
            "tools/call" => handle_tools_call(msg.get("params")),
            _ => {
                write_error(&mut stdout_lock, id.clone(), -32601, "Method not found");
                continue;
            }
        };

        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });
        let _ = writeln!(stdout_lock, "{}", response);
        let _ = stdout_lock.flush();
    }
}

fn handle_initialize() -> serde_json::Value {
    serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "companion", "version": "0.1.0" }
    })
}

fn handle_tools_list() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "companion_say",
                "description": "Make Pane (the Companion) speak a short message in a speech bubble. Use this to acknowledge the user, react to a finding, or celebrate a success — keep it under 10 words.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "The text to speak (under 10 words recommended)." },
                        "durationMs": { "type": "number", "description": "How long to show the bubble (default 3000 ms)." }
                    },
                    "required": ["text"]
                }
            },
            {
                "name": "companion_react",
                "description": "Trigger a brief animated reaction on Pane. Useful for non-verbal acknowledgement.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "emotion": {
                            "type": "string",
                            "enum": ["happy", "celebrate", "think", "confused", "concerned", "wave"],
                            "description": "The emotion to animate."
                        }
                    },
                    "required": ["emotion"]
                }
            },
            {
                "name": "companion_show_status",
                "description": "Reserved for future use — currently a no-op that acknowledges the call.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "status": { "type": "string" } }
                }
            }
        ]
    })
}

fn handle_tools_call(params: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(params) = params else {
        return tool_result_text("missing params");
    };
    let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(serde_json::Value::Null);

    // Forward as an MCP-framed event to the local IPC server. The JS side
    // keys off `type: "mcp"` and dispatches based on the inner tool name.
    let body = serde_json::json!({
        "type": "mcp",
        "payload": { "tool": name, "arguments": args }
    }).to_string();

    let ok = post_to_ipc(&body);
    let status = if ok { "ok" } else { "companion unreachable" };
    tool_result_text(status)
}

fn tool_result_text(text: &str) -> serde_json::Value {
    serde_json::json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
    })
}

fn write_error<W: Write>(w: &mut W, id: Option<serde_json::Value>, code: i32, message: &str) {
    let response = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    });
    let _ = writeln!(w, "{}", response);
    let _ = w.flush();
}

fn post_to_ipc(body: &str) -> bool {
    let addr = format!("127.0.0.1:{}", PORT);
    let Ok(addr) = addr.parse() else { return false; };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) else {
        return false;
    };
    let request = format!(
        "POST /event HTTP/1.1\r\n\
         Host: 127.0.0.1:{port}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\r\n{body}",
        port = PORT,
        len = body.len(),
        body = body,
    );
    let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }

    // Actually validate the response. Previously we returned "ok" purely on
    // a successful write, which gave a false positive whenever the server
    // wasn't listening but some other process (or the OS itself under
    // certain timing) accepted the connection briefly. Read the first line
    // of the HTTP response and require a 2xx status code.
    let _ = stream.set_read_timeout(Some(Duration::from_millis(1000)));
    let mut buf = [0u8; 64];
    let Ok(n) = stream.read(&mut buf) else { return false; };
    if n < 12 { return false; }
    let status_line = &buf[..n];
    // "HTTP/1.1 2XX" — any 2xx counts.
    status_line.starts_with(b"HTTP/1.1 2") || status_line.starts_with(b"HTTP/1.0 2")
}
