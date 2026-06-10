//! Minimal MCP stdio server (JSON-RPC 2.0).
//! Reads from stdin, writes to stdout, delegates to CLI commands.

use std::io::{self, BufRead, Write};
use serde_json::{Value, json};
use super::tools::all_tools;
use super::watcher;
use super::claude_md;
use crate::config::Config;

pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    // ── Detect project root ──
    let project_root = Config::detect_project_root();

    // ── Inject CLAUDE.md instructions ──
    if let Some(ref root) = project_root {
        match claude_md::inject_claude_md(root) {
            Ok(true) => eprintln!("[codexray] Injected codexray instructions into CLAUDE.md"),
            Ok(false) => {} // already present
            Err(e) => eprintln!("[codexray] CLAUDE.md injection skipped: {}", e),
        }
    }

    // ── Start background file watcher for auto-indexing ──
    let _watcher = match project_root.clone() {
        Some(ref root) => match watcher::start_watcher(root.clone()) {
            Ok(handle) => {
                eprintln!("[codexray] Watching {} for changes...", root.display());
                Some(handle)
            }
            Err(e) => {
                eprintln!("[codexray] Watcher unavailable: {}", e);
                None
            }
        },
        None => {
            eprintln!("[codexray] No project root found (no .git/). Watching disabled.");
            None
        }
    };

    // ── Run initial index in background on startup ──
    if let Some(root) = project_root {
        tokio::task::spawn_blocking(move || {
            eprintln!("[codexray] Running initial index for {}...", root.display());
            match run_cli(&["init"]) {
                Ok(_) => eprintln!("[codexray] Initial index complete."),
                Err(e) => eprintln!("[codexray] Initial index warning: {}", e),
            }
        });
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = request.get("id").cloned();

        let response = match method {
            "initialize" => handle_initialize(id),
            "notifications/initialized" => None,
            "tools/list" => handle_tools_list(id),
            "tools/call" => handle_tools_call(id, &request),
            _ => {
                Some(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {}", method)
                    }
                }))
            }
        };

        if let Some(resp) = response {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }

    Ok(())
}

fn handle_initialize(id: Option<Value>) -> Option<Value> {
    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "codexray",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": concat!(
                "Code intelligence MCP server — AST-based call graph + hybrid BM25+vector search.\n",
                "The index is built automatically on startup and updated on file changes.\n",
                "\n",
                "TOOL SELECTION (strict priority order):\n",
                "1. codexray_explore — FIRST for \"how does X work\" questions\n",
                "2. codexray_search  — FIRST for finding code by behavior/purpose\n",
                "3. codexray_find    — FIRST for finding code by name\n",
                "4. codexray_callers — REQUIRED before modifying/deleting any function\n",
                "5. codexray_callees — to understand internal dependencies\n",
                "6. Grep — ONLY for exact string literals (error messages, UUIDs, log formats)\n",
                "7. Glob — ONLY when you already know the exact filename\n",
                "\n",
                "WRONG: user asks \"how does indexing work\" → grep/find → read 10 files manually\n",
                "RIGHT: user asks \"how does indexing work\" → codexray_explore(\"code indexing\") → immediate answer"
            )
        }
    }))
}

fn handle_tools_list(id: Option<Value>) -> Option<Value> {
    let tools = all_tools();
    Some(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": tools
        }
    }))
}

fn handle_tools_call(id: Option<Value>, request: &Value) -> Option<Value> {
    let params = request.get("params")?;
    let tool_name = params.get("name")?.as_str()?;
    let default_args = json!({});
    let arguments = params.get("arguments").unwrap_or(&default_args);

    let result = match tool_name {
        "codexray_search" | "codexray_find" => {
            let query = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = arguments.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
            run_cli(&["search", query, "--limit", &limit.to_string(), "--json"])
        }
        "codexray_explore" => {
            let query = arguments.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = arguments.get("limit").and_then(|v| v.as_u64()).unwrap_or(5);
            run_cli(&["explore", query, "--limit", &limit.to_string()])
        }
        "codexray_callers" => {
            let symbol = arguments.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            run_cli(&["callers", symbol, "--json"])
        }
        "codexray_callees" => {
            let symbol = arguments.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
            run_cli(&["callees", symbol, "--json"])
        }
        "codexray_list" => {
            run_cli(&["list", "--json"])
        }
        "codexray_status" => {
            run_cli(&["status", "--json"])
        }
        _ => Err(format!("Unknown tool: {}", tool_name)),
    };

    match result {
        Ok(output) => {
            let content = if let Ok(parsed) = serde_json::from_str::<Value>(&output) {
                json!([{ "type": "text", "text": serde_json::to_string_pretty(&parsed).unwrap_or(output) }])
            } else {
                json!([{ "type": "text", "text": output }])
            };
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "content": content }
            }))
        }
        Err(e) => {
            Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": format!("Error: {}", e) }],
                    "isError": true
                }
            }))
        }
    }
}

/// Run the codexray CLI binary and capture its stdout.
fn run_cli(args: &[&str]) -> Result<String, String> {
    let bin = std::env::current_exe()
        .map_err(|e| format!("Failed to get binary path: {}", e))?;

    let output = std::process::Command::new(&bin)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run codexray: {}", e))?;

    if output.status.success() {
        String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 output: {}", e))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stderr.is_empty() && stdout.is_empty() {
            Err(format!("codexray exited with {}", output.status))
        } else if stderr.is_empty() {
            Err(format!("codexray exited with {}: {}", output.status, stdout.trim()))
        } else {
            Err(format!("codexray exited with {}: {}", output.status, stderr.trim()))
        }
    }
}
