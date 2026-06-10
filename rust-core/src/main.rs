use clap::Parser;
use codexray::cli::{Cli, CodeXRayRunner};
use codexray::cli::args::Commands;
use codexray::config::Config;
use codexray::mcp;
use tracing::info;
use std::path::PathBuf;

/// 从当前工作目录检测项目根（向上找 .git/）
fn detect_project() -> Result<PathBuf, String> {
    Config::detect_project_root()
        .ok_or_else(|| "No project found. Run codexray from within a git repository.".to_string())
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
                eprintln!("Use 'codexray serve --mcp' for MCP stdio mode.");
                return Ok(());
            }
            mcp::server::run_mcp_server().await?;
        }
        Commands::Install => {
            install_to_claude()?;
            install_to_codex()?;
        }
        Commands::Uninstall => {
            uninstall_from_claude()?;
            uninstall_from_codex()?;
        }
        Commands::InstallHooks => {
            let project_root = detect_project()?;
            let git_dir = project_root.join(".git");
            if !git_dir.exists() {
                return Err("Not a git repository.".into());
            }

            let hooks_dir = git_dir.join("hooks");
            std::fs::create_dir_all(&hooks_dir)?;

            let hook_script = "#!/bin/sh\n# Codexray auto-index hook\ncodexray init\n";

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
            println!("Each hook runs 'codexray init' for incremental indexing.");
        }
    }

    Ok(())
}

// ── MCP Install / Uninstall helpers ────────────────────────────────────

fn codexray_bin() -> String {
    "codexray".to_string()
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

fn claude_settings_path(local: bool) -> PathBuf {
    let base = if local {
        PathBuf::from(".claude")
    } else {
        dirs::home_dir().unwrap_or_default().join(".claude")
    };
    base.join("settings.json")
}

fn codex_config_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".codex").join("config.toml")
}

fn install_to_claude() -> Result<(), Box<dyn std::error::Error>> {
    let local = claude_local_mcp_path();
    let (mcp_path, settings_path, _scope) = if std::env::current_dir()
        .map(|d| d.join(".mcp.json").exists())
        .unwrap_or(false)
        || local.exists()
    {
        (local, claude_settings_path(true), "local")
    } else {
        (claude_global_mcp_path(), claude_settings_path(false), "global")
    };

    // 1. Write MCP server entry
    let mut mcp_config: serde_json::Value = if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !mcp_config.get("mcpServers").is_some() {
        mcp_config["mcpServers"] = serde_json::json!({});
    }
    mcp_config["mcpServers"]["codexray"] = mcp_server_entry();

    if let Some(parent) = mcp_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&mcp_path, serde_json::to_string_pretty(&mcp_config)?)?;
    println!("  MCP config -> {}", mcp_path.display());

    // 2. Write permissions
    let mut settings: serde_json::Value = if settings_path.exists() {
        let content = std::fs::read_to_string(&settings_path)?;
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    if !settings.get("permissions").is_some() {
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
    settings["permissions"]["allow"] = serde_json::json!(new_allow);

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&settings_path, serde_json::to_string_pretty(&settings)?)?;
    println!("  Permissions -> {}", settings_path.display());
    println!();
    println!("  Restart Claude Code to apply. codexray tools will appear automatically.");

    Ok(())
}

fn install_to_codex() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = codex_config_path();
    if !config_path.parent().map(|p| p.exists()).unwrap_or(false) {
        return Ok(());
    }

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
    } else {
        std::fs::create_dir_all(config_path.parent().unwrap())?;
        let content = if existing.is_empty() {
            toml_block
        } else {
            format!("{}\n\n{}", existing.trim_end(), toml_block)
        };
        std::fs::write(&config_path, content)?;
    }

    println!("  Codex config -> {}", config_path.display());
    Ok(())
}

fn uninstall_from_claude() -> Result<(), Box<dyn std::error::Error>> {
    let mcp_path = claude_global_mcp_path();
    if mcp_path.exists() {
        let content = std::fs::read_to_string(&mcp_path)?;
        let mut config: serde_json::Value = serde_json::from_str(&content)?;
        if config.get("mcpServers").and_then(|s| s.get("codexray")).is_some() {
            config["mcpServers"].as_object_mut().map(|s| s.remove("codexray"));
            std::fs::write(&mcp_path, serde_json::to_string_pretty(&config)?)?;
            println!("  Removed from {}", mcp_path.display());
        }
    }
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
            println!("  Removed from {}", config_path.display());
        }
    }
    Ok(())
}
