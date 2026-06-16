# CodeXray Improvement Plan

> Based on comprehensive analysis of the codebase (1409 functions, 78 files), June 2026.

## Executive Summary

CodeXray is a code search & knowledge engine built natively for Claude Code MCP protocol. Core capabilities include AST call graph, hybrid BM25+vector search with RRF fusion and Cross-Encoder reranking, real-time incremental indexing, and 7-language support.

This document identifies 10 improvement areas across security, performance, correctness, reliability, and UX, ranked by priority.

---

## P0 — Critical Correctness Issues

### 1. Search Results Line Number Information Lost in RRF Fusion

**Location**: `rust-core/src/services/hybrid_search.rs:83-94`

**Problem**: `DenseMeta::from` hardcodes `line_start` to 0 instead of using the actual line numbers from `SearchResult`:

```rust
impl From<&SearchResult> for DenseMeta {
    fn from(sr: &SearchResult) -> Self {
        let line_count = sr.code_block.lines().count();
        Self {
            file_path: sr.file_path.clone(),
            symbol_name: sr.symbol_name.clone(),
            code_block: sr.code_block.clone(),
            line_start: 0,       // ← should be sr.line_start
            line_end: line_count, // ← should be sr.line_end
        }
    }
}
```

LanceDB stores `line_start`/`line_end` in the vector table, and `SearchResult` carries these fields from the dense search channel. But when converting to `DenseMeta` for RRF fusion, the values are discarded. The final `FusedCandidate` always reports `line_start: 0`, making it impossible for users to navigate to the correct source location.

**Impact**: All hybrid search results via MCP tools (`codexray_search`, `codexray_find`) show `line_start: 0`. This directly undermines the tool's primary value — helping users locate code.

**Fix**:

```rust
impl From<&SearchResult> for DenseMeta {
    fn from(sr: &SearchResult) -> Self {
        Self {
            file_path: sr.file_path.clone(),
            symbol_name: sr.symbol_name.clone(),
            code_block: sr.code_block.clone(),
            line_start: sr.line_start,
            line_end: sr.line_end,
        }
    }
}
```

**Verification**: After fix, `codexray_search` results should show correct line numbers matching the source file.

---

### 2. Test Failure: `test_caching` Assertion Error

**Location**: `rust-core/src/services/embedding_service.rs:848-882`

**Problem**: The test asserts `call_count == 1` after first `process_file_content` call, but gets `0`. Root cause: the test uses `EmbeddingService::new_with_provider()` which creates a service with `dimensions: 2560` hardcoded, but also shares a global `EmbeddingCache` at `~/.codexray/cache/embedding_cache.sqlite`.

When multiple tests run in parallel, one test's cache insertion becomes visible to another. The `test_caching` test's "first call" may hit cache entries left by `test_search` or `test_duplicate_prevention` running concurrently, causing `call_count` to remain `0`.

Additionally, `process_file_content` first checks cache before calling the provider. If a hash collision or stale entry exists from a prior test run, the provider is never called.

**Impact**: CI pipeline has a flaky test. `cargo test` currently shows 42 passed, 1 failed.

**Fix**: Isolate the cache per test instance:

```rust
#[tokio::test]
async fn test_caching() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let db_path = dir.path().to_str().unwrap();
    let table_name = "test_caching".to_string();

    let mock_provider = MockEmbeddingProvider::new();
    let call_count = mock_provider.call_count.clone();
    let provider = Arc::new(mock_provider);

    let service = EmbeddingService::new_with_provider(db_path, table_name.clone(), provider).await?;
    // Override cache with isolated instance
    let isolated_cache = Arc::new(EmbeddingCache::new(
        dir.path().join("cache.sqlite").to_str().unwrap()
    )?);
    // ... need a way to inject isolated cache into service
    service.ensure_collection().await?;
    // ... rest of test
}
```

This requires either:
1. Making `EmbeddingService.cache` mutable (add a `set_cache` method for testing)
2. Adding a `new_with_provider_and_cache()` constructor
3. Using a test-specific cache directory via `Config::cache_dir()` override

**Verification**: `cargo test` should pass all 43 tests consistently, even with `-- --test-threads=1` and default parallel mode.

---

## P1 — Performance & Protocol Compliance

### 3. MCP Server Spawns Subprocess for Every Tool Call

**Location**: `rust-core/src/mcp/server.rs:344-369`

**Problem**: Each MCP `tools/call` request is handled by spawning a child process (`run_cli`) that executes `codexray search/callers/callees/etc. --json`. This means every tool call:
1. Forks a new process (OS overhead: ~10-50ms)
2. Cold-starts `EmbeddingService` (connect LanceDB, init SQLite cache)
3. Cold-starts `TantivyBm25Index` (open Tantivy reader)
4. Cold-starts `HybridSearchService` (assemble all components)
5. Generates embedding for the query (API call)
6. Performs search, serializes JSON, exits

For a typical `codexray_search` call, the total latency is: fork + cold-start + API call + search. The cold-start alone can be 100-500ms, on top of the actual search time.

Meanwhile, the MCP server already has a long-running process with tokio runtime, file watcher, and initial index built. None of this state is reused for tool calls.

**Impact**: Every MCP tool call has unnecessary 200-1000ms overhead. In a typical Claude Code session, a user might trigger 10-30 tool calls, adding 2-30 seconds of pure overhead.

**Fix**: Keep service instances alive in the MCP server process:

```rust
// In run_mcp_server(), after initial index:
struct McpState {
    project_hash: String,
    project_root: PathBuf,
    hybrid_search: Option<Arc<HybridSearchService>>,
    graph: Option<PetCodeGraph>,
    config: Config,
}

// Build HybridSearchService once, reuse for every search call
let state = Arc::new(McpState { ... });

// In handle_tools_call, use state directly instead of run_cli()
fn handle_tools_call(state: &McpState, id: Option<Value>, request: &Value) -> Option<Value> {
    match tool_name {
        "codexray_search" => {
            // Use state.hybrid_search directly — no subprocess
            let results = state.hybrid_search.search(query, limit).await;
            // Format and return
        }
        "codexray_callers" => {
            // Use state.graph.get_callers() directly
        }
        // ...
    }
}
```

**Trade-offs**:
- Pro: Eliminates subprocess overhead; search latency drops by 50-80%
- Pro: Graph and search state are always warm in memory
- Con: MCP server memory usage increases (graph + search indices in RAM)
- Con: Need to handle config changes without restart (watcher could signal reload)

**Migration Path**: Start by in-memory graph for callers/callees/explore (cheap, ~1-5MB). Add hybrid search later after memory profiling.

---

### 4. MCP Server Handles Requests Serially (No Concurrency)

**Location**: `rust-core/src/mcp/server.rs:53-89`

**Problem**: The MCP server main loop is synchronous:

```rust
for line in stdin.lock().lines() {
    let line = line?;
    // ... parse JSON-RPC
    let response = match method { ... };  // blocks until complete
    writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
    stdout.flush()?;
}
```

MCP protocol allows clients to send multiple requests concurrently. Claude Code may send `tools/call` requests in rapid succession. The current implementation blocks on each request — if `codexray_search` takes 5 seconds (API call + reranking), all subsequent requests wait.

**Impact**: When Claude Code issues multiple tool calls in one session step, they queue up instead of running in parallel. This is especially painful with the subprocess approach (#3) — each call blocks for fork + cold-start + search.

**Fix**: Use tokio's async stdin reading + spawn tasks:

```rust
pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    // ... setup watcher, initial index

    let stdin = tokio::io::BufReader::tokio(io::stdin());
    let mut stdout = io::stdout();
    let state = Arc::new(McpState { ... });

    let mut lines = stdin.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() { continue; }
        let request: Value = serde_json::from_str(&line)?;
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        // For notifications, handle immediately
        if method == "notifications/initialized" { continue; }

        // For tools/call, spawn async task
        let state_clone = state.clone();
        let response = tokio::spawn(async move {
            handle_request_async(&state_clone, id, &request).await
        }).await?;

        if let Some(resp) = response {
            writeln!(stdout, "{}", serde_json::to_string(&resp)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}
```

**Note**: This change depends on #3 (in-memory state) since concurrent subprocess spawning would cause resource contention. Implement #3 first, then #4.

---

### 5. File Watcher Triggers Re-index on Any File Change (No Extension Filter)

**Location**: `rust-core/src/mcp/watcher.rs:158-174`

**Problem**: `should_trigger_reindex` only checks:
1. Event type (Modify/Data, Create, Remove) — correct
2. Directory exclusion (.git, node_modules, target, etc.) — correct

But it does NOT check file extensions. Any file change in a non-excluded directory triggers `codexray init`, including:
- `.log` files (log rotation)
- `.tmp` / `.bak` files (editor temp files)
- `.svg` / `.png` / `.ico` (assets)
- `.md` / `.txt` / `.json` (non-code, unless `.json` is config)
- `.lock` files (package lock changes)
- `.toml` / `.yaml` (config files, not parsed by tree-sitter)

On a typical project, these non-code file changes happen frequently (editor saves, log writes, lock file updates). Each triggers a full `codexray init` subprocess.

**Impact**: Unnecessary re-indexing wastes CPU, API credits (embedding calls), and time. In worst case, a log file rotating every minute triggers re-index every 8 seconds after each rotation event.

**Fix**: Add supported extension whitelist:

```rust
/// File extensions that CodeXray indexes (matching supported languages).
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "c", "cpp", "cc", "cxx",
    "h", "hpp", "hh", "java", "jav",
];

fn should_trigger_reindex(event: &Event) -> bool {
    match &event.kind {
        EventKind::Modify(ModifyKind::Data(_)) => {}
        EventKind::Create(_) | EventKind::Remove(_) => {}
        _ => return false,
    }

    for path in &event.paths {
        if is_in_excluded_dir(path) { continue; }
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if SUPPORTED_EXTENSIONS.contains(&ext) {
            return true;
        }
    }
    false
}
```

**Trade-offs**:
- Pro: Eliminates spurious re-indexing
- Con: Adding a new language requires updating both tree-sitter parser AND watcher whitelist
- Mitigation: Move `SUPPORTED_EXTENSIONS` to a shared constant or config

---

## P2 — Security & Stability

### 6. LanceDB Predicate Injection Risk

**Location**: `rust-core/src/services/embedding_service.rs:402-408`

**Problem**: `delete_file_embeddings` constructs a SQL-like predicate via string interpolation:

```rust
async fn delete_file_embeddings(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let table = self.connection.open_table(&self.table_name).execute().await?;
    let predicate = format!("file_path = '{}'", file_path.replace("'", "''"));
    table.delete(&predicate).await?;
    Ok(())
}
```

The `file_path` comes from `PathBuf.to_string_lossy()` which is derived from filesystem paths. While single-quote escaping (`''`) handles the most obvious injection vector, LanceDB's predicate syntax may support:
- Escape sequences (`\`, `\\'`)
- Unicode-based injection
- Expression operators (`OR`, `AND`, `;`)

Since `file_path` is user-controlled (it comes from scanned file paths which could include attacker-controlled filenames), this is a theoretical injection vector.

**Impact**: Low probability, high severity. An attacker could craft a filename like `file' OR '1'='1.rs` to delete all embeddings.

**Fix**: Use LanceDB's safer API if available, or apply more thorough sanitization:

```rust
fn sanitize_lance_predicate(value: &str) -> String {
    // Escape all characters that could break LanceDB predicate syntax
    let sanitized = value
        .replace('\\', "\\\\")
        .replace("'", "\\'")
        .replace('"', "\\\"");
    format!("file_path = '{}'", sanitized)
}
```

Or better, check if LanceDB supports parameterized predicates in newer versions and switch to that API.

---

### 7. Embedding Cache Has No Eviction Policy

**Location**: `rust-core/src/services/embedding_service.rs:28-68`

**Problem**: `EmbeddingCache` (SQLite-backed) has:
- No maximum size limit
- No TTL/expiration on entries
- No LRU eviction
- A global shared file at `~/.codexray/cache/embedding_cache.sqlite`

Over time (months of usage across multiple projects), the cache grows indefinitely. Each embedding is a 2560-dimension float32 vector (10KB) serialized via bincode. For a project with 1000 functions, the cache stores ~10MB. For 10 projects over 6 months, it could reach 100MB+.

Worse: stale entries for deleted/renamed code are never cleaned, so the cache contains entries that will never be looked up again (hash includes the code content, so renamed files get new hashes but old hashes stay).

**Impact**: Disk space bloat; potential SQLite performance degradation on large caches; stale entries waste lookup time.

**Fix**: Add periodic eviction in `EmbeddingCache`:

```rust
impl EmbeddingCache {
    /// Evict entries older than `max_age_days` days.
    fn evict_stale(&self, max_age_days: u64) -> Result<usize, Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        let cutoff = chrono::Utc::now().timestamp() - (max_age_days as i64 * 86400);
        let deleted = conn.execute(
            "DELETE FROM embedding_cache WHERE created_at < ?1",
            params![cutoff],
        )?;
        Ok(deleted)
    }

    /// Evict entries to keep cache under `max_entries`.
    fn evict_oversized(&self, max_entries: usize) -> Result<usize, Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM embedding_cache", [], |row| row.get(0)
        )?;
        if count <= max_entries as i64 { return Ok(0); }
        let to_delete = count - max_entries as i64;
        conn.execute(
            "DELETE FROM embedding_cache WHERE rowid IN (
                SELECT rowid FROM embedding_cache ORDER BY created_at ASC LIMIT ?1
            )",
            params![to_delete],
        )?;
        Ok(to_delete as usize)
    }
}
```

Call eviction periodically (e.g., every 100 cache inserts, or on service init).

**Config Addition**:

```json
{
  "embedding": {
    "cache_max_age_days": 90,
    "cache_max_entries": 50000
  }
}
```

---

### 8. Reranker Lacks Robustness for Large Candidate Pools

**Location**: `rust-core/src/services/reranker_service.rs:63-142`

**Problem**: The reranker sends all candidate `code_block` strings as `documents` in a single API request:

```rust
let documents: Vec<String> = candidates.iter().map(|c| c.code_block.clone()).collect();
let top_n = self.config.top_n.min(candidates.len());
```

Issues:
1. **Request size limit**: If each code_block is ~500 lines, 20 candidates = 10,000 lines. The 4B reranker model has a context window limit (typically 32K tokens). Sending too much content may cause truncation or API errors.
2. **No retry/backoff**: A single HTTP failure falls back to RRF results silently. No exponential backoff for transient failures (rate limits, timeouts).
3. **No truncation strategy**: Long code blocks are sent verbatim. No smart truncation (e.g., signature + first 20 lines).
4. **Timeout mismatch**: Default `timeout_secs: 30` but the config says `60` in README example. The actual default is 30 (`default_30`), inconsistent with documentation.

**Impact**: Large projects may hit API size limits; transient network errors cause silent degradation; documentation is misleading.

**Fix**:

1. Truncate long code blocks before sending to reranker:

```rust
fn truncate_for_rerank(code: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = code.lines().collect();
    if lines.len() <= max_lines { return code.to_string(); }
    // Keep signature (first few lines) + tail
    let sig_lines = min(5, max_lines / 2);
    let tail_lines = max_lines - sig_lines;
    format!(
        "{}\n... ({} lines truncated) ...\n{}",
        lines[..sig_lines].join("\n"),
        lines.len() - sig_lines - tail_lines,
        lines[lines.len()-tail_lines..].join("\n"),
    )
}
```

2. Add retry with backoff:

```rust
async fn rerank_with_retry(&self, query: &str, candidates: Vec<FusedCandidate>) -> Result<Vec<FusedCandidate>> {
    let max_retries = 2;
    for attempt in 0..=max_retries {
        match self.rerank(query, candidates.clone()).await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < max_retries => {
                let delay = Duration::from_secs(2u64.pow(attempt));
                warn!("Reranker attempt {} failed: {}, retrying in {}s", attempt, e, delay.as_secs());
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
```

3. Fix documentation: align README default `timeout_secs: 60` with code default `30`, or change code to match README.

---

## P3 — UX & Search Quality

### 9. Explore Command: Name-First, Semantic-Fallback Design Is Suboptimal

**Location**: `rust-core/src/cli/runner.rs:322-428`

**Problem**: The `codexray_explore` MCP tool works in two phases:
1. `graph.find_functions_by_name(&query)` — substring match on function names
2. If empty, fallback to `run_cli_search(&query, limit * 2)` — fork subprocess for semantic search

For natural-language queries like "how does authentication work" or "payment processing logic":
- Phase 1 almost always returns empty (no function named "authentication work")
- Phase 2 forks a subprocess (cold-start overhead per #3)
- Two-step sequential flow adds latency

The tool description says "FIRST for 'how does X work' questions" but its implementation prioritizes exact name matching over semantic understanding.

**Impact**: Explore's most valuable use case (natural-language queries) has the worst performance path.

**Fix**: Parallelize both channels, merge results:

```rust
// Phase 1: Run both in parallel
let (name_results, semantic_results) = tokio::join!(
    async { graph.find_functions_by_name(&query) },
    hybrid_search.search(&query, limit),
);

// Phase 2: Merge — prefer name matches (high confidence), supplement with semantic
let mut output = Vec::new();
let seen_ids: HashSet<Uuid> = HashSet::new();

// Add name matches first
for func in &name_results {
    seen_ids.insert(func.id);
    output.push(explore_entry(&graph, func));
}

// Add semantic matches not already covered
for candidate in semantic_results {
    let funcs = graph.find_functions_by_name(&candidate.symbol_name);
    for func in funcs {
        if !seen_ids.contains(&func.id) {
            seen_ids.insert(func.id);
            output.push(explore_entry(&graph, func));
        }
    }
}

output.truncate(limit);
```

This gives name matches priority (they're exact) while enriching with semantic discoveries, all in one pass.

---

### 10. Setup Wizard Hard-Codes Single Provider Default

**Location**: `rust-core/src/config.rs:84-85`, `rust-core/src/cli/runner.rs:853`

**Problem**: The default embedding API is hardcoded to SiliconFlow:

```rust
fn default_api_base_url() -> String { "https://api.siliconflow.cn/v1".to_string() }
fn default_model() -> String { "Qwen/Qwen3-Embedding-4B".to_string() }
```

And the setup wizard only offers one base URL default. Users who prefer OpenAI, OpenRouter, local vLLM, or other providers must manually edit `~/.codexray/config.json`.

The reranker setup wizard (runner.rs:886-917) already has a multi-provider `Select` menu with 4 options. But the embedding setup has no equivalent.

**Impact**: Non-SiliconFlow users face friction in setup; the product appears China-specific.

**Fix**: Add provider selection to the embedding setup wizard, mirroring the reranker pattern:

```rust
let embedding_providers = vec![
    "SiliconFlow (Qwen3-Embedding-4B)     — api.siliconflow.cn",
    "OpenAI (text-embedding-3-small)       — api.openai.com",
    "OpenRouter (multi-model)              — openrouter.ai",
    "Local vLLM / Ollama                   — localhost:11434",
    "Custom provider",
];
let provider_idx = Select::new()
    .with_prompt("Choose an embedding provider")
    .items(&embedding_providers)
    .default(0)
    .interact()?;

let (default_url, default_model, default_dims) = match provider_idx {
    0 => ("https://api.siliconflow.cn/v1", "Qwen/Qwen3-Embedding-4B", 2560),
    1 => ("https://api.openai.com/v1", "text-embedding-3-small", 1536),
    2 => ("https://openrouter.ai/api/v1", "openai/text-embedding-3-small", 1536),
    3 => ("http://localhost:11434/v1", "nomic-embed-text", 768),
    _ => (config.embedding.api_base_url.as_str(), config.embedding.model.as_str(), config.embedding.dimensions),
};
```

This also requires making `dimensions` configurable per provider, since different models have different vector sizes.

---

## Implementation Roadmap

### Phase 1 (Quick Wins) — 1-2 days

| # | Item | Effort | Risk |
|---|------|--------|------|
| 1 | Fix line_start in DenseMeta | 1 line change | None |
| 2 | Fix test_caching isolation | Small refactor | Low |
| 5 | Watcher extension filter | Add constant + check | None |
| 6 | LanceDB predicate sanitization | Small helper function | Low |

### Phase 2 (Architecture) — 3-5 days

| # | Item | Effort | Risk |
|---|------|--------|------|
| 3 | MCP in-memory service instances | Medium refactor | Medium (memory lifecycle) |
| 4 | MCP async request handling | Medium refactor | Depends on #3 |
| 8 | Reranker truncation + retry | Medium | Low |

### Phase 3 (Polish) — 2-3 days

| # | Item | Effort | Risk |
|---|------|--------|------|
| 7 | Cache eviction policy | Medium | Low |
| 9 | Explore parallel channels | Medium | Low |
| 10 | Multi-provider setup wizard | Medium | Low |

### Total estimated effort: 6-10 days

---

## Appendix: Current Test Status

```
42 passed, 1 failed (services::embedding_service::tests::test_caching)
```

The failing test is a cache isolation issue, not a functional bug. All AST parser tests pass across 7 languages. MCP watcher tests pass. CLAUDE.md injection tests pass.
