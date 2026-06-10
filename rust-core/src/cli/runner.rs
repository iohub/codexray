use crate::config::Config;
use crate::storage::StorageManager;
use crate::storage::lock::FileLock;
use crate::services::CodeAnalyzer;
use crate::services::EmbeddingService;
use crate::services::RerankerService;
use crate::services::hybrid_search::HybridSearchService;
use crate::storage::TantivyBm25Index;
use crate::codegraph::types::PetCodeGraph;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

use super::args::{Cli, Commands};

/// 从当前工作目录检测项目根（向上找 .git/）
fn detect_project() -> Result<PathBuf, String> {
    Config::detect_project_root()
        .ok_or_else(|| "No project found. Run codexray from within a git repository.".to_string())
}

/// 获取项目索引目录和锁路径
fn project_paths(project_root: &PathBuf) -> (PathBuf, PathBuf) {
    let hash = Config::compute_project_hash(project_root);
    let index_dir = Config::project_index_dir(&hash);
    let lock_path = index_dir.join(".lock");
    (index_dir, lock_path)
}

pub struct CodeXRayRunner;

impl CodeXRayRunner {
    pub fn new() -> Self {
        Self
    }

    pub async fn run(cli: Cli, config: Option<Config>) -> Result<(), Box<dyn std::error::Error>> {
        match cli.command {
            Commands::Init { interactive: _ } => {
                let project_root = detect_project()?;
                let (index_dir, lock_path) = project_paths(&project_root);
                let _lock = FileLock::exclusive(lock_path)?;

                eprintln!("  Project:    {}", project_root.display());
                eprintln!("  Index dir:  {}", index_dir.display());
                eprintln!();

                let project_hash = Config::compute_project_hash(&project_root);
                let storage = Arc::new(StorageManager::new());
                if let Some(ref cfg) = config {
                    storage.set_config(cfg.clone());
                }

                // Try loading existing graph for incremental update
                let existing_graph = storage.get_persistence().load_graph(&project_hash).ok().flatten();

                // Phase 1: Parse files
                eprintln!("  Scanning & parsing source files...");
                let mut analyzer = CodeAnalyzer::new();
                let code_graph = analyzer.analyze_directory(&project_root)
                    .map_err(|e| format!("Analysis failed: {}", e))?;
                let new_stats = code_graph.get_stats();
                eprintln!("  Found {} files, {} functions", new_stats.total_files, new_stats.total_functions);

                // Phase 2: Build graph
                eprintln!("  Building call graph...");
                let mut pet_graph = existing_graph.unwrap_or_else(|| PetCodeGraph::new());

                if new_stats.total_functions > 0 {
                    // Rebuild graph from fresh analysis (simpler than incremental)
                    pet_graph = PetCodeGraph::new();
                    for func in code_graph.functions.values() {
                        pet_graph.add_function(func.clone());
                    }
                    for rel in &code_graph.call_relations {
                        let _ = pet_graph.add_call_relation(rel.clone());
                    }
                    pet_graph.update_stats();
                }

                // Phase 3: Save graph
                eprintln!("  Saving call graph...");
                let meta = serde_json::json!({
                    "project_root": project_root.to_string_lossy(),
                    "indexed_at": chrono::Utc::now().to_rfc3339(),
                });
                std::fs::create_dir_all(&index_dir)?;
                std::fs::write(index_dir.join("project.json"), serde_json::to_string_pretty(&meta)?)?;

                let stats = pet_graph.get_stats().clone();
                storage.get_persistence().save_graph(&project_hash, &pet_graph)?;
                storage.set_graph(pet_graph);

                // Phase 4: Embedding (optional, only if API token configured)
                let mut embedding_done = false;
                if let Some(ref cfg) = config {
                    if !cfg.embedding.api_token.is_empty() {
                        eprintln!("  Building vector embeddings...");
                        let db_path = Config::lancedb_dir(&project_hash).to_string_lossy().to_string();
                        let collection = format!("codexray_{}", &project_hash[..8]);

                        let bm25_dir = Config::bm25_dir(&project_hash);
                        let bm25_index = TantivyBm25Index::open_or_create(&bm25_dir)
                            .ok()
                            .map(|idx| Arc::new(idx) as Arc<dyn crate::storage::traits_bm25::TextSearchProvider>);

                        embedding_done = true;
                        match EmbeddingService::new(&db_path, collection, Some(cfg), bm25_index).await {
                            Ok(es) => {
                                if let Err(e) = es.ensure_collection().await {
                                    warn!("Embedding table setup failed: {}", e);
                                    embedding_done = false;
                                } else {
                                    let embedding_hashes_path = index_dir.join("embedding_hashes.json");
                                    let existing_hashes: Option<std::collections::HashMap<String, String>> = std::fs::read_to_string(&embedding_hashes_path)
                                        .ok()
                                        .and_then(|s| serde_json::from_str(&s).ok());

                                    match es.vectorize_directory(
                                        &project_root.to_string_lossy(),
                                        existing_hashes.as_ref(),
                                    ).await {
                                        Ok(new_hashes) => {
                                            if let Ok(json) = serde_json::to_string_pretty(&new_hashes) {
                                                let _ = std::fs::write(&embedding_hashes_path, json);
                                            }
                                        }
                                        Err(e) => {
                                            warn!("Embedding not available: {}. Graph-based search will be used.", e);
                                            embedding_done = false;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Embedding service unavailable: {}. Graph-based search will be used.", e);
                                embedding_done = false;
                            }
                        }
                    }
                }

                let suffix = if embedding_done { " + embeddings" } else { "" };
                eprintln!("  Done: {} files, {} functions{}", stats.total_files, stats.total_functions, suffix);
            }
            Commands::Status { json } => {
                let project_root = detect_project()?;
                let (index_dir, _) = project_paths(&project_root);
                let project_hash = Config::compute_project_hash(&project_root);

                let storage = Arc::new(StorageManager::new());
                let status = match storage.get_persistence().load_graph(&project_hash) {
                    Ok(Some(graph)) => {
                        let stats = graph.get_stats();
                        serde_json::json!({
                            "project_root": project_root,
                            "project_hash": project_hash,
                            "index_dir": index_dir,
                            "total_functions": stats.total_functions,
                            "total_files": stats.total_files,
                            "indexed": true,
                        })
                    }
                    _ => {
                        serde_json::json!({
                            "project_root": project_root,
                            "project_hash": project_hash,
                            "indexed": false,
                        })
                    }
                };

                if json {
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    println!("Project:     {:?}", project_root);
                    println!("Indexed:     {}", status["indexed"]);
                    if status["indexed"].as_bool().unwrap_or(false) {
                        println!("Functions:   {}", status["total_functions"]);
                        println!("Files:       {}", status["total_files"]);
                    }
                }
            }
            Commands::Search { query, limit, json } => {
                let project_root = detect_project()?;
                let (_index_dir, lock_path) = project_paths(&project_root);
                let _lock = FileLock::shared(lock_path)?;
                let project_hash = Config::compute_project_hash(&project_root);

                let results = if let Some(ref cfg) = config {
                    if !cfg.embedding.api_token.is_empty() {
                        let collection = format!("codexray_{}", &project_hash[..8]);
                        let db_path = Config::lancedb_dir(&project_hash).to_string_lossy().to_string();

                        if let Ok(es) = EmbeddingService::new(&db_path, collection, Some(cfg), None).await {
                            let bm25_dir = Config::bm25_dir(&project_hash);
                            let bm25_index = TantivyBm25Index::open_or_create(&bm25_dir)
                                .ok()
                                .map(|idx| Arc::new(idx) as Arc<dyn crate::storage::traits_bm25::TextSearchProvider>);

                            if let Some(bm25) = bm25_index {
                                let hybrid_cfg = &cfg.index.hybrid;
                                let reranker_cfg = &cfg.index.reranker;

                                let hybrid = if reranker_cfg.enabled && !reranker_cfg.api_token.is_empty() {
                                    let reranker = RerankerService::new(reranker_cfg.clone());
                                    HybridSearchService::with_reranker(
                                        Arc::new(es),
                                        bm25,
                                        hybrid_cfg.into(),
                                        Some(reranker),
                                    )
                                } else {
                                    HybridSearchService::new(
                                        Arc::new(es),
                                        bm25,
                                        hybrid_cfg.into(),
                                    )
                                };
                                Some(hybrid.search(&query, limit).await.unwrap_or_default())
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Fallback to graph-based name search
                let use_graph_fallback = results.as_ref().map(|r| r.is_empty()).unwrap_or(true);

                if use_graph_fallback {
                    let storage = Arc::new(StorageManager::new());
                    if let Ok(Some(graph)) = storage.get_persistence().load_graph(&project_hash) {
                        let funcs = graph.find_functions_by_name(&query);
                        if json {
                            let output: Vec<_> = funcs.iter().map(|f| serde_json::json!({
                                "name": f.name,
                                "file_path": f.file_path,
                                "line_start": f.line_start,
                                "line_end": f.line_end,
                                "language": f.language,
                            })).collect();
                            println!("{}", serde_json::to_string_pretty(&output)?);
                        } else if funcs.is_empty() {
                            println!("No results found.");
                        } else {
                            for (i, f) in funcs.iter().enumerate().take(limit) {
                                println!("{}. {} [{}]", i + 1, f.name, f.language);
                                println!("   {}:{}", f.file_path.display(), f.line_start);
                            }
                        }
                    } else {
                        println!("No index found. Run 'codexray init' first.");
                    }
                } else {
                    let results = results.unwrap();
                    if json {
                        println!("{}", serde_json::to_string_pretty(&results)?);
                    } else if results.is_empty() {
                        println!("No results found.");
                    } else {
                        for (i, r) in results.iter().enumerate() {
                            println!("{}. {} ({:.4})", i + 1, r.symbol_name, r.final_score);
                            println!("   {}:{}", r.file_path, r.line_start);
                        }
                    }
                }
            }
            Commands::Callers { symbol, json } => {
                let project_root = detect_project()?;
                let (_, lock_path) = project_paths(&project_root);
                let _lock = FileLock::shared(lock_path)?;
                let project_hash = Config::compute_project_hash(&project_root);
                let storage = Arc::new(StorageManager::new());

                match storage.get_persistence().load_graph(&project_hash) {
                    Ok(Some(graph)) => {
                        let results = execute_callers(&graph, &symbol, json)?;
                        if !json && results.trim().is_empty() {
                            println!("No callers found for '{}'", symbol);
                        } else {
                            print!("{}", results);
                        }
                    }
                    _ => {
                        println!("No index found. Run 'codexray init' first.");
                    }
                }
            }
            Commands::Callees { symbol, json } => {
                let project_root = detect_project()?;
                let (_, lock_path) = project_paths(&project_root);
                let _lock = FileLock::shared(lock_path)?;
                let project_hash = Config::compute_project_hash(&project_root);
                let storage = Arc::new(StorageManager::new());

                match storage.get_persistence().load_graph(&project_hash) {
                    Ok(Some(graph)) => {
                        let results = execute_callees(&graph, &symbol, json)?;
                        if !json && results.trim().is_empty() {
                            println!("No callees found for '{}'", symbol);
                        } else {
                            print!("{}", results);
                        }
                    }
                    _ => {
                        println!("No index found. Run 'codexray init' first.");
                    }
                }
            }
            Commands::Uninit { force } => {
                let project_root = detect_project()?;
                let (index_dir, lock_path) = project_paths(&project_root);

                if !index_dir.exists() {
                    println!("No index found for this project.");
                    return Ok(());
                }

                if !force {
                    print!("Delete index for {:?}? [y/N] ", project_root);
                    io::stdout().flush()?;
                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    if !input.trim().eq_ignore_ascii_case("y") {
                        println!("Aborted.");
                        return Ok(());
                    }
                }

                let _lock = FileLock::exclusive(lock_path)?;
                std::fs::remove_dir_all(&index_dir)?;
                info!("Deleted index at {:?}", index_dir);
                println!("Index deleted: {:?}", project_root);

                if let Ok(ref mut cfg) = Config::load() {
                    let key = project_root.to_string_lossy().to_string();
                    cfg.installed_hooks.remove(&key);
                    cfg.save().ok();
                }
            }
            Commands::List { json } => {
                let projects_dir = Config::projects_dir();
                if !projects_dir.exists() {
                    println!("No indexed projects found.");
                    return Ok(());
                }

                let mut projects = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&projects_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            let meta_file = path.join("project.json");
                            if meta_file.exists() {
                                if let Ok(content) = std::fs::read_to_string(&meta_file) {
                                    if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&content) {
                                        let hash = path.file_name().unwrap_or_default().to_string_lossy();
                                        let graph_file = path.join("graph.bin");
                                        let functions = if graph_file.exists() {
                                            let s = graph_file.metadata().map(|m| m.len()).unwrap_or(0);
                                            format!("{}", s)
                                        } else { "—".to_string() };
                                        projects.push(serde_json::json!({
                                            "project_root": meta["project_root"],
                                            "hash": hash,
                                            "indexed_at": meta.get("indexed_at").map(|v| v.as_str().unwrap_or("")),
                                            "size": functions,
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }

                if json {
                    println!("{}", serde_json::to_string_pretty(&projects)?);
                } else if projects.is_empty() {
                    println!("No indexed projects found.");
                } else {
                    for p in &projects {
                        println!("  {}  →  {}", p["hash"].as_str().unwrap_or("?").chars().take(12).collect::<String>(), p["project_root"].as_str().unwrap_or("?"));
                    }
                }
            }
            Commands::Serve { mcp } => {
                if !mcp {
                    eprintln!("Use 'codexray serve --mcp' for MCP stdio mode.");
                    return Ok(());
                }
                crate::mcp::server::run_mcp_server().await?;
            }
            Commands::Install { local, global } => {
                let scope = resolve_scope(local, global);
                install_to_claude(scope)?;
                install_to_codex()?;
            }
            Commands::Uninstall { local, global } => {
                let scope = resolve_scope(local, global);
                uninstall_from_claude(scope)?;
                uninstall_from_codex()?;
            }
            Commands::Daemon { background } => {
                if background {
                    // Fork to background
                    daemonize()?;
                }

                let project_root = detect_project()?;
                let (index_dir, _) = project_paths(&project_root);

                eprintln!("╔══════════════════════════════════════════════╗");
                eprintln!("║       CodeXray Daemon                        ║");
                eprintln!("╠══════════════════════════════════════════════╣");
                eprintln!("║  Project:   {:<34}║", project_root.display().to_string().chars().take(34).collect::<String>());
                eprintln!("║  Index:     {:<34}║", index_dir.display().to_string().chars().take(34).collect::<String>());
                eprintln!("║  PID:       {:<34}║", std::process::id());
                eprintln!("╚══════════════════════════════════════════════╝");
                eprintln!();
                eprintln!("[codexray-daemon] Watching for file changes...");
                eprintln!("[codexray-daemon] Auto-indexing on changes (8s debounce).");
                eprintln!();

                // Run initial index
                eprintln!("[codexray-daemon] Running initial index...");
                if let Err(e) = run_index_sync(&project_root) {
                    eprintln!("[codexray-daemon] Initial index failed: {}", e);
                } else {
                    eprintln!("[codexray-daemon] Initial index complete.");
                }

                // Start file watcher and block forever
                match crate::mcp::watcher::start_watcher(project_root.clone()) {
                    Ok(_handle) => {
                        eprintln!("[codexray-daemon] Watcher started for {}", project_root.display());

                        // Block forever — watcher handles re-indexing
                        loop {
                            std::thread::park();
                        }
                        // Note: handle is never dropped here, so watcher runs forever
                        // In practice, SIGTERM/SIGINT will kill the process
                    }
                    Err(e) => {
                        return Err(format!("Failed to start watcher: {}", e).into());
                    }
                }
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
}

fn execute_callers(
    graph: &PetCodeGraph,
    symbol: &str,
    json: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let functions = graph.find_functions_by_name(symbol);
    let mut output = String::new();

    if json {
        let mut results = Vec::new();
        for func in &functions {
            for (caller, relation) in graph.get_callers(&func.id) {
                results.push(serde_json::json!({
                    "caller": caller.name,
                    "caller_file": caller.file_path,
                    "caller_line": relation.line_number,
                    "target": func.name,
                }));
            }
        }
        output = serde_json::to_string_pretty(&results)?;
    } else {
        for func in &functions {
            let callers = graph.get_callers(&func.id);
            if callers.is_empty() {
                output.push_str(&format!("No callers for '{}'\n", func.name));
            } else {
                output.push_str(&format!("Callers of '{}':\n", func.name));
                for (caller, relation) in callers {
                    output.push_str(&format!(
                        "  {} ({}:{})\n",
                        caller.name,
                        caller.file_path.display(),
                        relation.line_number
                    ));
                }
            }
        }
    }

    if functions.is_empty() {
        if json {
            output = "[]".to_string();
        }
    }

    Ok(output)
}

fn execute_callees(
    graph: &PetCodeGraph,
    symbol: &str,
    json: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let functions = graph.find_functions_by_name(symbol);
    let mut output = String::new();

    if json {
        let mut results = Vec::new();
        for func in &functions {
            for (callee, relation) in graph.get_callees(&func.id) {
                results.push(serde_json::json!({
                    "callee": callee.name,
                    "callee_file": callee.file_path,
                    "callee_line": relation.line_number,
                    "caller": func.name,
                }));
            }
        }
        output = serde_json::to_string_pretty(&results)?;
    } else {
        for func in &functions {
            let callees = graph.get_callees(&func.id);
            if callees.is_empty() {
                output.push_str(&format!("No callees for '{}'\n", func.name));
            } else {
                output.push_str(&format!("Callees of '{}':\n", func.name));
                for (callee, relation) in callees {
                    output.push_str(&format!(
                        "  {} ({}:{})\n",
                        callee.name,
                        callee.file_path.display(),
                        relation.line_number
                    ));
                }
            }
        }
    }

    if functions.is_empty() {
        if json {
            output = "[]".to_string();
        }
    }

    Ok(output)
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
    println!("Installing codexray MCP server to Claude Code ({})...\n", scope);

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
    println!("  CodeXRay MCP server registered for Claude Code.");
    println!("  Restart Claude Code to apply. The following tools become available:\n");
    println!("    codexray_search   — semantic code search");
    println!("    codexray_callers  — find callers of a symbol");
    println!("    codexray_callees  — find callees of a symbol");
    println!("    codexray_init     — build/update code index");
    println!("    codexray_status   — index health check");
    println!("    codexray_list     — list indexed projects");

    Ok(())
}

fn install_to_codex() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = codex_config_path();
    if !config_path.parent().map(|p| p.exists()).unwrap_or(false) {
        return Ok(());
    }

    println!("\nInstalling codexray MCP server to Codex CLI...\n");

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
    println!("  CodeXRay MCP server registered for Codex CLI.");
    Ok(())
}

fn uninstall_from_claude(scope: Scope) -> Result<(), Box<dyn std::error::Error>> {
    println!("Removing codexray MCP server from Claude Code ({})...\n", scope);

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
        println!("  [skip] No codexray entry in {}", mcp_path.display());
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
    println!("  CodeXRay MCP server unregistered from Claude Code.");
    Ok(())
}

/// Daemonize the process (Unix: double-fork to background).
#[cfg(unix)]
fn daemonize() -> Result<(), Box<dyn std::error::Error>> {
    // First fork
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err("fork failed".into());
    }
    if pid > 0 {
        // Parent exits
        std::process::exit(0);
    }

    // Create new session
    if unsafe { libc::setsid() } < 0 {
        return Err("setsid failed".into());
    }

    // Second fork
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err("second fork failed".into());
    }
    if pid > 0 {
        std::process::exit(0);
    }

    // Close standard file descriptors
    unsafe {
        libc::close(0);
        libc::close(1);
        libc::close(2);
        // Redirect to /dev/null
        let devnull = libc::open(b"/dev/null\0" as *const u8 as *const libc::c_char, libc::O_RDWR);
        libc::dup2(devnull, 0);
        libc::dup2(devnull, 1);
        libc::dup2(devnull, 2);
        libc::close(devnull);
    }

    Ok(())
}

#[cfg(not(unix))]
fn daemonize() -> Result<(), Box<dyn std::error::Error>> {
    Err("Background mode is only supported on Unix platforms.".into())
}

/// Run `codexray init` synchronously for the given project.
fn run_index_sync(project_root: &std::path::Path) -> Result<(), String> {
    let bin = std::env::current_exe().map_err(|e| format!("Cannot find binary: {}", e))?;

    let output = std::process::Command::new(&bin)
        .arg("init")
        .current_dir(project_root)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("Failed to run codexray init: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("codexray init failed: {}", stderr.trim()))
    }
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
            println!("  CodeXRay MCP server unregistered from Codex CLI.");
        } else {
            println!("  [skip] No codexray entry in {}", config_path.display());
        }
    }
    Ok(())
}
