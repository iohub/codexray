//! Minimal MCP (Model Context Protocol) stdio server.
//!
//! When invoked as `codexray serve --mcp`, reads JSON-RPC 2.0 messages
//! from stdin and responds on stdout. Exposes the CLI commands as MCP
//! tools that Claude Code / Codex can discover and call.

pub mod claude_md;
pub mod server;
pub mod tools;
pub mod watcher;
