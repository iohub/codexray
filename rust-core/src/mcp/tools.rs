use serde::Serialize;

/// MCP tool definition.
#[derive(Serialize, Clone)]
pub struct Tool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// All codexray MCP tools.
pub fn all_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "codexray_search".into(),
            description: "Find functions, classes, or methods by name or natural language description. This is the PRIMARY code search tool — use it INSTEAD of grep, Glob, or Grep for locating code symbols. It understands code semantics (not just text matching), so you can search by what the code DOES (e.g., \"parse JSON\", \"handle authentication\") or by symbol name. Returns ranked results with signatures.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to find — a function name, class name, method name, or natural language description of the functionality (e.g., \"token validation\", \"login handler\", \"parseConfig\", \"database connection pool\")"
                    },
                    "limit": {
                        "type": "number",
                        "description": "Maximum number of results to return (default: 10)",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "codexray_callers".into(),
            description: "Find ALL call sites of a function, method, or class — every function that depends on this symbol. Use this BEFORE modifying or deleting any function to assess impact. Unlike grep, this traces actual AST-level call relationships, not text matches.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "The function name, method name, or class name to analyze (e.g., \"handleRequest\", \"UserService.login\")"
                    }
                },
                "required": ["symbol"]
            }),
        },
        Tool {
            name: "codexray_callees".into(),
            description: "Find ALL functions, methods, or classes called BY the given symbol. Use this to understand a function's dependencies — what external services, utilities, or helpers it relies on. AST-level tracing, not text matching.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "The function name, method name, or class name to analyze (e.g., \"handleRequest\", \"UserService.login\")"
                    }
                },
                "required": ["symbol"]
            }),
        },
        Tool {
            name: "codexray_list".into(),
            description: "List all projects currently indexed by codexray. Use this to discover which codebases are available for search and analysis.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "codexray_status".into(),
            description: "Check the health and freshness of the codexray index — number of indexed functions, files, last index time. Use this to verify the index is up-to-date before relying on search results.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}
