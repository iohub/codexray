use serde::Serialize;

/// MCP tool definition.
#[derive(Serialize, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// All CodeXray MCP tools.
///
/// Design principle: each description leads with a TRIGGER ("When user asks X...")
/// not a feature description. This ensures LLMs pattern-match the tool to the task
/// before reaching for Grep/Glob/Bash.
///
/// Tool count is intentionally small (6 tools) to minimize cognitive overhead.
/// codexray_search + codexray_find cover all code-location needs via the same
/// hybrid search backend.
pub fn all_tools() -> Vec<Tool> {
    vec![
        // ── Primary search tools ────────────────────────────────────
        Tool {
            name: "codexray_search".into(),
            description: concat!(
                "Your FIRST choice for understanding or exploring code. ",
                "Use INSTEAD of Grep/Glob when finding code by what it DOES (not literal text). ",
                "Understands synonyms: \"auth\" matches \"login\", \"authenticate\", \"verify credentials\". ",
                "Returns function/class definitions with signatures, file paths, and line numbers — ",
                "no follow-up reads needed.\n\n",
                "TRIGGERS (use BEFORE Grep/Glob):\n",
                "- \"how does X work\" / \"explain Y\"\n",
                "- \"find where Z happens\"\n",
                "- Any code exploration or investigation task"
            ).into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Describe what the code DOES in natural language. Good: \"how requests are authenticated\", \"payment processing logic\". Bad: regex patterns, exact strings."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum results (default: 10).",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "codexray_find".into(),
            description: concat!(
                "Find code symbols by NAME. Use INSTEAD of Glob or Bash find when you know ",
                "the function/class/method name you're looking for. Returns AST-verified ",
                "DEFINITIONS (not text matches) with file paths and line numbers.\n\n",
                "TRIGGERS:\n",
                "- \"find the login handler\"\n",
                "- \"where is UserService defined\"\n",
                "- \"look for processPayment function\""
            ).into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Symbol name or natural description (e.g., \"error handling middleware\", \"rate limiter\", \"UserSchema\")."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum results (default: 10).",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "codexray_explore".into(),
            description: concat!(
                "DEFAULT first step for \"how does X work?\" / \"explain the architecture\" questions. ",
                "Combines search + call graph: returns the key functions for a concept, ",
                "with each function's callers and callees. Gives you a ready-made map of ",
                "the relevant code paths.\n\n",
                "USE THIS when the user asks to EXPLORE, EXPLAIN, UNDERSTAND, or INVESTIGATE ",
                "how something works in the codebase. It is always the right tool for ",
                "architecture questions."
            ).into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The concept or feature to explore (e.g., \"code indexing\", \"authentication flow\", \"request pipeline\")."
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum functions to explore (default: 5).",
                        "default": 5
                    }
                },
                "required": ["query"]
            }),
        },
        // ── Call graph tools ────────────────────────────────────────
        Tool {
            name: "codexray_callers".into(),
            description: concat!(
                "Find ALL functions that CALL this symbol. REQUIRED before modifying or deleting ",
                "any function — shows every downstream consumer and the full blast radius. ",
                "AST-level tracing (not text matching), so it won't miss indirect references ",
                "or confuse same-named symbols in different scopes."
            ).into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Function/method name (e.g., \"handleRequest\", \"UserService.login\")."
                    }
                },
                "required": ["symbol"]
            }),
        },
        Tool {
            name: "codexray_callees".into(),
            description: concat!(
                "Find ALL functions CALLED BY this symbol: its internal dependencies. ",
                "Use to understand what a function relies on — databases, APIs, utilities, helpers. ",
                "AST-level tracing, accurate even with same-named symbols in different scopes."
            ).into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "Function/method name (e.g., \"handleRequest\", \"UserService.login\")."
                    }
                },
                "required": ["symbol"]
            }),
        },
        // ── Meta / utility tools ────────────────────────────────────
        Tool {
            name: "codexray_status".into(),
            description: "Check index freshness — number of indexed functions, files, last update timestamp. Use to verify the index is ready before relying on search or call-graph results.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}
