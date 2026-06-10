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
            description: "语义代码搜索 — 通过自然语言或符号名称查找代码。结合向量嵌入和 BM25 全文检索，返回最相关的函数、类或方法。用于快速定位代码位置。".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "搜索查询文本或符号名称（如 \"authentication\", \"handleRequest\", \"user login\"）"
                    },
                    "limit": {
                        "type": "number",
                        "description": "最大返回结果数（默认: 10）",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "codexray_callers".into(),
            description: "调用者查询 — 列出哪些函数调用了指定符号。用于理解上游依赖和影响分析。".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "要查询的函数、方法或类名"
                    }
                },
                "required": ["symbol"]
            }),
        },
        Tool {
            name: "codexray_callees".into(),
            description: "被调用者查询 — 列出指定符号调用了哪些函数。用于理解函数的依赖关系。".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "symbol": {
                        "type": "string",
                        "description": "要查询的函数、方法或类名"
                    }
                },
                "required": ["symbol"]
            }),
        },
        Tool {
            name: "codexray_init".into(),
            description: "构建或更新当前项目的代码索引。首次使用其他 codexray 工具前必须先运行此命令。幂等操作 — 后续运行仅重新处理已更改的文件。".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "codexray_list".into(),
            description: "列出所有已被 codexray 索引的项目。返回项目根路径列表。".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "codexray_status".into(),
            description: "索引健康检查 — 显示当前项目的函数数、文件数、最后索引时间等信息。".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}
