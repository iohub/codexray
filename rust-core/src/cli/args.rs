use clap::{Parser, Subcommand, ValueEnum};

/// 存储方式配置
#[derive(Debug, Clone, ValueEnum)]
pub enum StorageMode {
    /// 仅JSON格式存储
    Json,
    /// 仅二进制格式存储
    Binary,
    /// 同时保存JSON和二进制格式
    Both,
}

impl Default for StorageMode {
    fn default() -> Self {
        StorageMode::Binary
    }
}

#[derive(Parser, Debug)]
#[clap(name = "codexray", author, version, about = "Code intelligence CLI tool", long_about = None)]
pub struct Cli {
    /// Verbose mode
    #[clap(short, long, action)]
    pub verbose: bool,

    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build or update the code index (full on first run, MD5-incremental thereafter)
    Init {
        /// Interactive configuration wizard
        #[clap(short = 'i', long, action)]
        interactive: bool,
    },
    /// Show index statistics (functions, files, last update)
    Status {
        /// Output as JSON
        #[clap(long, action)]
        json: bool,
    },
    /// Semantic code search (vector + BM25 + RRF fusion)
    Search {
        /// Search query text
        query: String,
        /// Maximum results to return
        #[clap(short, long, default_value = "10")]
        limit: usize,
        /// Output as JSON
        #[clap(long, action)]
        json: bool,
    },
    /// Find functions that call the given symbol
    Callers {
        /// Function or symbol name
        symbol: String,
        /// Output as JSON
        #[clap(long, action)]
        json: bool,
    },
    /// Find functions called by the given symbol
    Callees {
        /// Function or symbol name
        symbol: String,
        /// Output as JSON
        #[clap(long, action)]
        json: bool,
    },
    /// Delete the current project's index data
    Uninit {
        /// Skip confirmation prompt
        #[clap(long, action)]
        force: bool,
    },
    /// List all indexed projects
    List {
        /// Output as JSON
        #[clap(long, action)]
        json: bool,
    },
    /// Start MCP server (for Claude Code / Codex integration)
    Serve {
        /// Run in MCP stdio mode
        #[clap(long, action)]
        mcp: bool,
    },
    /// Register codexray as MCP tools in Claude Code / Codex
    Install {
        /// Use local .mcp.json (project-level, not global ~/.claude.json)
        #[clap(long, action)]
        local: bool,
        /// Use global ~/.claude.json (user-level, default)
        #[clap(long, action)]
        global: bool,
    },
    /// Remove codexray MCP integration from Claude Code / Codex
    Uninstall {
        /// Remove from local .mcp.json only
        #[clap(long, action)]
        local: bool,
        /// Remove from global ~/.claude.json only (default)
        #[clap(long, action)]
        global: bool,
    },
    /// Install git hooks (post-commit, post-merge → codexray init)
    InstallHooks,
}
