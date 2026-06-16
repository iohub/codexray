use clap::Parser;
use codexray::cli::{Cli, CodeXRayRunner};
use codexray::cli::args::Commands;
use codexray::config::Config;
use codexray::mcp;
use tracing::info;
use std::path::PathBuf;
use console::style;
use dialoguer::{Confirm, Input, Select};
use indicatif::{ProgressBar, ProgressStyle};

/// 从当前工作目录检测项目根（向上找 .git/）
fn detect_project() -> Result<PathBuf, String> {
    Config::detect_project_root()
        .ok_or_else(|| "No project found. Run CodeXray from within a git repository.".to_string())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let filter_layer = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            if cli.verbose {
                tracing_subscriber::EnvFilter::new("debug")
            } else {
                tracing_subscriber::EnvFilter::new("warn")
            }
        });
    tracing_subscriber::fmt().with_env_filter(filter_layer).init();

    let config = Config::load().ok();

    match &cli.command {
        Commands::Init { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::Status { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::Search { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::Explore { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::Callers { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::Callees { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::Uninit { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::List { .. } => {
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::Serve { mcp } => {
            if !mcp {
                eprintln!("Use 'CodeXray serve --mcp' for MCP stdio mode.");
                return Ok(());
            }
            mcp::server::run_mcp_server().await?;
        }
        Commands::Install { local, global } => {
            initialize_codexray_dir()?;
            let scope = resolve_scope(*local, *global);
            install_to_claude(scope)?;
            install_to_codex()?;
        }
        Commands::Uninstall { local, global } => {
            let scope = resolve_scope(*local, *global);
            uninstall_from_claude(scope)?;
            uninstall_from_codex()?;
        }
        Commands::Daemon { background } => {
            if *background && !cfg!(unix) {
                eprintln!("Background mode is only supported on Unix platforms.");
                return Ok(());
            }
            CodeXRayRunner::run(cli, config).await?;
        }
        Commands::InstallHooks => {
            let project_root = detect_project()?;
            let git_dir = project_root.join(".git");
            if !git_dir.exists() {
                return Err("Not a git repository.".into());
            }

            let hooks_dir = git_dir.join("hooks");
            std::fs::create_dir_all(&hooks_dir)?;

            let hook_script = "#!/bin/sh\n# CodeXray auto-index hook\ncodexray init\n";

            for hook_name in &["post-commit", "post-merge"] {
                let hook_path = hooks_dir.join(hook_name);
                std::fs::write(&hook_path, hook_script)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&hook_path, std::fs::Permissions::from_mode(0o755))?;
                }
                info!("Installed git hook: {:?}", hook_path);
            }

            if let Ok(mut cfg) = Config::load() {
                cfg.installed_hooks.insert(
                    project_root.to_string_lossy().to_string(),
                    vec!["post-commit".into(), "post-merge".into()],
                );
                cfg.save().ok();
            }

            println!("Git hooks installed: post-commit, post-merge");
            println!("Each hook runs 'CodeXray init' for incremental indexing.");
        }
    }

    Ok(())
}

// ── MCP Install / Uninstall helpers ────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Local,
    Global,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Local => write!(f, "local (.mcp.json)"),
            Scope::Global => write!(f, "global (~/.claude.json)"),
        }
    }
}

fn resolve_scope(local: bool, _global: bool) -> Scope {
    if local {
        Scope::Local
    } else {
        Scope::Global
    }
}

fn codexray_bin() -> String {
    let bin_name = if cfg!(target_os = "windows") {
        "codexray.exe"
    } else {
        "codexray"
    };
    Config::bin_dir()
        .join(bin_name)
        .to_string_lossy()
        .to_string()
}

/// 初始化 ~/.codexray 目录结构：创建目录、复制二进制、引导用户配置 — CodeXray
fn initialize_codexray_dir() -> Result<(), Box<dyn std::error::Error>> {
    let codexray_dir = Config::codexray_dir();
    let bin_dir = Config::bin_dir();
    let projects_dir = Config::projects_dir();
    let cache_dir = Config::cache_dir();

    // ── 1. 创建目录结构 ──────────────────────────────────────
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::default_spinner()
        .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"])
        .template("{spinner:.green} {msg}")?);
    pb.set_message("Creating directories...");
    for dir in [&codexray_dir, &bin_dir, &projects_dir, &cache_dir] {
        std::fs::create_dir_all(dir)?;
    }
    pb.finish_with_message(format!("{} {}", style("✔").green(), style("Directories created").bold()));

    // ── 2. 复制二进制 ─────────────────────────────────────────
    let current_exe = std::env::current_exe()?;
    let dest_bin = bin_dir.join(if cfg!(target_os = "windows") { "codexray.exe" } else { "codexray" });

    if current_exe != dest_bin {
        let pb2 = ProgressBar::new_spinner();
        pb2.set_style(ProgressStyle::default_spinner()
            .tick_strings(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"])
            .template("{spinner:.green} {msg}")?);
        pb2.set_message("Installing binary...");
        std::fs::copy(&current_exe, &dest_bin)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dest_bin, std::fs::Permissions::from_mode(0o755))?;
        }
        pb2.finish_with_message(format!("{} Binary installed", style("✔").green()));
    } else {
        println!("{} {}", style("✔").green(), style("Binary already in place").dim());
    }

    // ── 3. 检查是否已有配置 ──────────────────────────────────
    let config_path = Config::global_config_path();
    if config_path.exists() {
        let reconfigure = Confirm::new()
            .with_prompt(format!("Config already exists at {}. Reconfigure?", config_path.display()))
            .default(false)
            .interact()?;
        if !reconfigure {
            println!("{} Keeping existing config.", style("✔").green());
            return Ok(());
        }
    }

    // ── 4. 交互式引导 ────────────────────────────────────────
    println!();
    println!("{}", style("╔══════════════════════════════════════╗").cyan());
    println!("{}", style("║       CodeXRay Interactive Setup     ║").cyan().bold());
    println!("{}", style("╚══════════════════════════════════════╝").cyan());
    println!();

    let mut config = if config_path.exists() {
        Config::load().unwrap_or_default()
    } else {
        Config::default()
    };

    // ── Embedding API 配置 ────────────────────────────────────
    println!("{}", style("── Step 1: Embedding Configuration ──").bold());
    println!();
    println!("  CodeXRay uses an embedding API for semantic code search.");
    println!("  Basic features (call graph, name search) work WITHOUT it.");
    println!();

    let setup_embedding = Confirm::new()
        .with_prompt("Configure embedding API for semantic search?")
        .default(true)
        .interact()?;

    if !setup_embedding {
        config.embedding.api_token = String::new();
        println!("{} Embedding disabled — graph-based search only.", style("➜").yellow());
        config.save()?;
        println!();
        println!("{} {}", style("✔").green(), style(format!("Configuration saved to {}", config_path.display())).bold());
        println!();
        return Ok(());
    }

    // ── API Token ─────────────────────────────────────────────
    let api_token: String = Input::<String>::new()
        .with_prompt("API Token")
        .interact_text()?;

    if api_token.trim().len() < 5 {
        println!("{} No valid API token provided — skipping embedding.", style("⚠").yellow());
        config.embedding.api_token = String::new();
        config.save()?;
        println!();
        println!("{} {}", style("✔").green(), style(format!("Configuration saved to {}", config_path.display())).bold());
        println!();
        return Ok(());
    }

    config.embedding.api_token = api_token.trim().to_string();
    config.embedding.provider = "openai-compatible".to_string();

    // ── API Base URL ──────────────────────────────────────────
    let api_base_url: String = Input::<String>::new()
        .with_prompt("API Base URL")
        .default("https://api.siliconflow.cn/v1".to_string())
        .interact_text()?;
    config.embedding.api_base_url = api_base_url.trim().to_string();

    // ── Model ─────────────────────────────────────────────────
    let model: String = Input::<String>::new()
        .with_prompt("Embedding model")
        .default("Qwen/Qwen3-Embedding-4B".to_string())
        .interact_text()?;
    config.embedding.model = model.trim().to_string();

    // ── Step 2: Reranker ──────────────────────────────────────
    println!();
    println!("{}", style("── Step 2: Reranker Configuration ──").bold());
    println!();
    println!("  A reranker improves search result quality by re-scoring candidates.");

    let enable_reranker = Confirm::new()
        .with_prompt("Enable reranker for better search quality?")
        .default(true)
        .interact()?;

    if enable_reranker {
        config.index.reranker.enabled = true;

        let reranker_token: String = Input::<String>::new()
            .with_prompt("Reranker API Token")
            .interact_text()?;

        if reranker_token.trim().is_empty() {
            config.index.reranker.enabled = false;
            println!("{} Reranker disabled — no token provided.", style("➜").yellow());
        } else {
            config.index.reranker.api_token = reranker_token.trim().to_string();

            let reranker_models = vec![
                "Qwen/Qwen3-Reranker-4B       (SiliconFlow)",
                "cohere/rerank-4-pro           (OpenRouter)",
                "BAAI/bge-reranker-v2-m3       (local/vLLM)",
                "Custom model",
            ];
            let reranker_idx = Select::new()
                .with_prompt("Choose a reranker model")
                .items(&reranker_models)
                .default(0)
                .interact()?;

            let (default_reranker_model, default_reranker_url) = match reranker_idx {
                0 => ("Qwen/Qwen3-Reranker-4B".to_string(), config.embedding.api_base_url.clone()),
                1 => ("cohere/rerank-4-pro".to_string(), "https://openrouter.ai/api/v1".to_string()),
                2 => ("BAAI/bge-reranker-v2-m3".to_string(), "https://api.siliconflow.cn/v1".to_string()),
                _ => (config.index.reranker.model.clone(), config.index.reranker.api_base_url.clone()),
            };

            let reranker_model: String = Input::<String>::new()
                .with_prompt("Reranker model")
                .default(default_reranker_model)
                .interact_text()?;
            config.index.reranker.model = reranker_model.trim().to_string();

            let reranker_url: String = Input::<String>::new()
                .with_prompt("Reranker API Base URL")
                .default(default_reranker_url)
                .interact_text()?;
            config.index.reranker.api_base_url = reranker_url.trim().to_string();
        }
    } else {
        config.index.reranker.enabled = false;
        println!("{} Reranker disabled.", style("➜").yellow());
    }

    // ── 5. 保存配置 ──────────────────────────────────────────
    config.save()?;
    println!();
    println!("{} {}", style("✔").green(), style(format!("Configuration saved to {}", config_path.display())).bold());
    println!("  Run {} to build your first index!", style("codexray init").cyan());
    println!();

    Ok(())
}

fn mcp_server_entry() -> serde_json::Value {
    serde_json::json!({
        "command": codexray_bin(),
        "args": ["serve", "--mcp"]
    })
}

fn claude_global_mcp_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude.json")
}

fn claude_local_mcp_path() -> PathBuf {
    PathBuf::from(".mcp.json")
}

fn claude_settings_path(scope: Scope) -> PathBuf {
    match scope {
        Scope::Local => PathBuf::from(".claude").join("settings.json"),
        Scope::Global => {
            dirs::home_dir().unwrap_or_default().join(".claude").join("settings.json")
        }
    }
}

fn codex_config_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex").join("config.toml")
}

fn install_to_claude(scope: Scope) -> Result<(), Box<dyn std::error::Error>> {
    println!("Installing CodeXray MCP server to Claude Code ({})...\n", scope);

    let (mcp_path, settings_path) = match scope {
        Scope::Local => (claude_local_mcp_path(), claude_settings_path(Scope::Local)),
        Scope::Global => (claude_global_mcp_path(), claude_settings_path(Scope::Global)),
    };

    // 1. Write MCP server entry
    let mut mcp_config: serde_json::Value = if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let existing_entry = mcp_config
        .get("mcpServers")
        .and_then(|s| s.get("codexray"));
    let is_update = existing_entry.is_some();

    if mcp_config.get("mcpServers").is_none() {
        mcp_config["mcpServers"] = serde_json::json!({});
    }
    mcp_config["mcpServers"]["codexray"] = mcp_server_entry();

    if let Some(parent) = mcp_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&mcp_path, serde_json::to_string_pretty(&mcp_config)?)?;

    if is_update {
        println!("  [update] MCP config: {}", mcp_path.display());
    } else {
        println!("  [create] MCP config: {}", mcp_path.display());
    }
    println!("    entry: {} serve --mcp", codexray_bin());

    // 2. Write permissions
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let had_permissions = settings.get("permissions").is_some();

    if !had_permissions {
        settings["permissions"] = serde_json::json!({});
    }
    let allow = settings["permissions"]["allow"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let perms = ["Bash(codexray *)"];
    let mut new_allow = allow.clone();
    for p in &perms {
        let s = p.to_string();
        if !allow.iter().any(|v| v.as_str() == Some(&s)) {
            new_allow.push(serde_json::json!(s));
        }
    }

    if new_allow.len() > allow.len() {
        settings["permissions"]["allow"] = serde_json::json!(new_allow);

        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
        println!("  [update] Permissions: {}", settings_path.display());
        println!("    added: Bash(codexray *)");
    } else {
        println!("  [skip] Permissions already configured: {}", settings_path.display());
    }


    println!();
    println!("  ✓ CodeXray MCP server registered for Claude Code.");
    println!("  Restart Claude Code to apply. The following tools become available:\n");
    println!("    codexray_explore  — explore a concept: search + callers + callees combined");
    println!("    codexray_search   — find code by behavior/purpose (semantic search)");
    println!("    codexray_find     — find code symbols by name");
    println!("    codexray_callers  — find all callers of a function/method");
    println!("    codexray_callees  — find all callees (dependencies) of a function");
    println!("    codexray_status   — index health check");

    Ok(())
}

fn install_to_codex() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = codex_config_path();
    if !config_path.parent().map(|p| p.exists()).unwrap_or(false) {
        // Codex CLI not found — skip silently
        return Ok(());
    }

    println!("\nInstalling CodeXray MCP server to Codex CLI...\n");

    let toml_block = format!(
        "[mcp_servers.codexray]\ncommand = \"{}\"\nargs = [\"serve\", \"--mcp\"]\n",
        codexray_bin()
    );

    let existing = if config_path.exists() {
        std::fs::read_to_string(&config_path)?
    } else {
        String::new()
    };

    let header = "[mcp_servers.codexray]";
    if existing.contains(header) {
        let start = existing.find(header).unwrap();
        let end = existing[start..]
            .find("\n[")
            .map(|i| start + i)
            .unwrap_or(existing.len());
        let mut updated = existing[..start].to_string();
        updated.push_str(&toml_block);
        if end < existing.len() {
            updated.push_str(&existing[end..]);
        }
        std::fs::write(&config_path, updated.trim_end())?;
        println!("  [update] Codex config: {}", config_path.display());
    } else {
        std::fs::create_dir_all(config_path.parent().unwrap())?;
        let content = if existing.is_empty() {
            toml_block
        } else {
            format!("{}\n\n{}", existing.trim_end(), toml_block)
        };
        std::fs::write(&config_path, content)?;
        println!("  [create] Codex config: {}", config_path.display());
    }
    println!("  ✓ CodeXray MCP server registered for Codex CLI.");
    Ok(())
}

fn uninstall_from_claude(scope: Scope) -> Result<(), Box<dyn std::error::Error>> {
    println!("Removing CodeXray MCP server from Claude Code ({})...\n", scope);

    let mcp_path = match scope {
        Scope::Local => claude_local_mcp_path(),
        Scope::Global => claude_global_mcp_path(),
    };

    let mut removed_mcp = false;
    if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path)?;
        let mut config: serde_json::Value = serde_json::from_str(&content)?;
        if config.get("mcpServers").and_then(|s| s.get("codexray")).is_some() {
            config["mcpServers"]
                .as_object_mut()
                .map(|s| s.remove("codexray"));
            std::fs::write(&mcp_path, serde_json::to_string_pretty(&config)?)?;
            println!("  [remove] MCP config: {}", mcp_path.display());
            removed_mcp = true;
        }
    }
    if !removed_mcp {
        println!("  [skip] No CodeXray entry in {}", mcp_path.display());
    }

    // Clean up permissions
    let settings_path = claude_settings_path(scope);
    if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        let mut settings: serde_json::Value = serde_json::from_str(&content)?;
        if let Some(allow) = settings["permissions"]["allow"].as_array_mut() {
            let before = allow.len();
            allow.retain(|v| v.as_str() != Some("Bash(codexray *)"));
            if allow.len() < before {
                std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
                println!("  [update] Permissions: removed Bash(codexray *)");
            }
        }
    }

    println!();
    println!("  ✓ CodeXray MCP server unregistered from Claude Code.");
    Ok(())
}

fn uninstall_from_codex() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = codex_config_path();
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let header = "[mcp_servers.codexray]";
        if content.contains(header) {
            let start = content.find(header).unwrap();
            let end = content[start..]
                .find("\n[")
                .map(|i| start + i)
                .unwrap_or(content.len());
            let mut updated = content[..start].to_string();
            if end < content.len() {
                updated.push_str(&content[end..]);
            }
            std::fs::write(&config_path, updated.trim())?;
            println!("  [remove] Codex config: {}", config_path.display());
            println!("  ✓ CodeXray MCP server unregistered from Codex CLI.");
        } else {
            println!("  [skip] No CodeXray entry in {}", config_path.display());
        }
    }
    Ok(())
}
