# CodeXRay

**Code intelligence MCP server for Claude Code & Codex CLI.** AST-based call graph analysis + hybrid semantic search (Dense + Sparse + RRF + Cross-Encoder Reranker) — right from your terminal.

## Quick Start

### Download Binary

Pre-built binaries are available for different platforms:

| Platform | Command |
|----------|---------|
| **macOS (Apple Silicon)** | `curl -L -o codexray.tar.gz https://github.com/superdl/codexray/releases/latest/download/codexray-darwin-arm64.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |
| **macOS (Intel)** | `curl -L -o codexray.tar.gz https://github.com/superdl/codexray/releases/latest/download/codexray-darwin-x64.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |
| **Linux (glibc)** | `curl -L -o codexray.tar.gz https://github.com/superdl/codexray/releases/latest/download/codexray-linux-x64.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |
| **Linux (musl)** | `curl -L -o codexray.tar.gz https://github.com/superdl/codexray/releases/latest/download/codexray-linux-x64-musl.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |

> **Note:** On first run, `codexray` will launch an interactive setup wizard to configure your embedding model and API token.

After `./codexray install`, restart Claude Code — it will auto-discover codexray's MCP tools (`codexray_explore`, `codexray_search`, `codexray_find`, `codexray_callers`, `codexray_callees`, `codexray_status`). The server auto-indexes your project on startup and gracefully exits when Claude Code closes — no daemon or manual commands needed.

## Install

### Binary (GitHub Releases)

Pre-built binaries for each release:

| Platform | Architecture | Download |
|----------|-------------|----------|
| macOS | Apple Silicon (arm64) | `codexray-darwin-arm64.tar.gz` |
| macOS | Intel (x64) | `codexray-darwin-x64.tar.gz` |
| Linux | x64 (glibc) | `codexray-linux-x64.tar.gz` |
| Linux | x64 (musl, static) | `codexray-linux-x64-musl.tar.gz` |

Download the archive for your platform from the [latest release](https://github.com/superdl/codexray/releases/latest), extract it, then run `codexray install` — it will automatically complete the setup:

```bash
# macOS (Apple Silicon)
curl -L -o codexray.tar.gz https://github.com/superdl/codexray/releases/latest/download/codexray-darwin-arm64.tar.gz
tar -xzf codexray.tar.gz
./codexray install

# macOS (Intel)
curl -L -o codexray.tar.gz https://github.com/superdl/codexray/releases/latest/download/codexray-darwin-x64.tar.gz
tar -xzf codexray.tar.gz
./codexray install

# Linux (x64 glibc)
curl -L -o codexray.tar.gz https://github.com/superdl/codexray/releases/latest/download/codexray-linux-x64.tar.gz
tar -xzf codexray.tar.gz
./codexray install
```

The `install` command registers codexray as MCP tools in Claude Code / Codex CLI. On first run, `codexray` will also launch an interactive setup wizard to configure your embedding model and API token.

### From source

```bash
git clone https://github.com/superdl/codexray.git
cd codexray
cargo build --release
```

The binary will be at `rust-core/target/release/codexray`. Run it directly:

```bash
./rust-core/target/release/codexray install
```

On first run, `./codexray install` will also launch the interactive setup wizard to configure your embedding model and API token.

## How It Works

### Index Building

```
Source files
  → Tree-sitter AST parse (7 languages)
  → Extract functions / classes / methods
  → Build call graph (PetCodeGraph)
  → Batch embed via API (SQLite cache)
  → Store vectors in LanceDB
  → Build BM25 index in Tantivy
  → Save to ~/.codexray/<project_hash>/
```

**Idempotent**: index builds are incremental — the first run is a full build, subsequent runs compare MD5 hashes and only re-process changed files. Use `codexray install-hooks` for automatic re-indexing on git commit/merge, or `codexray daemon` for real-time file watching.

### Hybrid Search Pipeline (`codexray search`)

```
                        ┌─────────────────────┐
User query ────────────→│  Embedding Model     │──→ Query vector
                        └─────────────────────┘
                                  │
          ┌───────────────────────┼───────────────────────┐
          ▼                       ▼                       ▼
   ┌─────────────┐       ┌─────────────┐       ┌─────────────┐
   │ Dense Search │       │ Sparse Search│       │ Graph Search │
   │ (LanceDB ANN)│       │ (Tantivy BM25)│      │ (PetCodeGraph)│
   └──────┬───────┘       └──────┬──────┘       └──────┬──────┘
          │                      │                      │
          └──────────────────────┼──────────────────────┘
                                 ▼
                        ┌─────────────────┐
                        │   RRF Fusion    │  ← Reciprocal Rank Fusion
                        │  (Top-20 candidates)│
                        └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │    Reranker     │  ← Cross-Encoder fine re-ranking
                        │ (Qwen3-Reranker)│     scores each (query, code) pair
                        └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │   Final Results  │  ← Top-5 (or Top-N)
                        └─────────────────┘
```

| Stage | Technology | Role |
|-------|-----------|------|
| **Dense Search** | LanceDB + Embedding Model | Semantic vector similarity |
| **Sparse Search** | Tantivy BM25 | Keyword & token matching |
| **RRF Fusion** | Reciprocal Rank Fusion | Merge heterogeneous scores fairly |
| **Reranker** | Cross-Encoder (Qwen3-Reranker-4B) | Full-interaction precision scoring |
| **Fallback** | PetCodeGraph | Graph-based name search (no API needed) |

If embedding/reranker are unavailable, the pipeline falls back gracefully to graph-based name search and BM25-only mode.

### Auto-Indexing Modes

| Mode | When | Trigger |
|------|------|---------|
| **Git hooks** | On commit/merge | `codexray install-hooks` |
| **Daemon** | On file change (8s debounce) | `codexray daemon` |
| **MCP server** | On startup + file changes | `codexray install` + restart Claude Code |

The MCP server automatically runs all three: initial index on startup, file watching during runtime, and CLAUDE.md injection for tool discovery.

### Storage

- **Config**: `~/.codexray/config.json` (global, shared across all projects)
- **Index**: `~/.codexray/<md5(project_root)>/`
  - `project.json` — Project metadata
  - `graph.bin` — Serialized call graph
  - `embeddings.lance/` — LanceDB vector data
  - `tantivy_bm25/` — BM25 full-text index
  - `file_hashes.json` — MD5 incremental tracking
  - `embedding_hashes.json` — Embedding incremental tracking

No daemon, no HTTP server (unless running `daemon` or `serve --mcp`). Every CLI command is a standalone process.

## Supported Languages

| Language | Functions | Structs/Classes | Call Graph |
|----------|:---------:|:---------------:|:----------:|
| Rust | ✅ | ✅ | ✅ |
| Python | ✅ | ✅ | ✅ |
| JavaScript | ✅ | ✅ | ✅ |
| TypeScript | ✅ | ✅ | ✅ |
| Go | ✅ | ✅ | ✅ |
| C/C++ | ✅ | ✅ | ✅ |
| Java | ✅ | ✅ | ✅ |

## Configuration

`~/.codexray/config.json`:

```json
{
  "embedding": {
    "provider": "openai-compatible",
    "model": "Qwen/Qwen3-Embedding-4B",
    "api_token": "sk-...",
    "api_base_url": "https://api.siliconflow.cn/v1",
    "dimensions": 2560
  },
  "index": {
    "min_code_block_length": 16,
    "enable_reranker": true,
    "hybrid": {
      "enable_bm25": true,
      "bm25_top_k": 100,
      "vector_top_k": 100,
      "rrf_k": 60,
      "rrf_top_k": 20,
      "short_code_threshold": 30,
      "short_code_penalty": 0.5
    },
    "reranker": {
      "enabled": true,
      "model": "Qwen/Qwen3-Reranker-4B",
      "api_token": "sk-...",
      "api_base_url": "https://api.siliconflow.cn/v1/rerank",
      "top_n": 5,
      "candidate_multiplier": 5,
      "timeout_secs": 60
    }
  },
  "installed_hooks": {}
}
```

### Model Roles

| Model | Role | When |
|-------|------|------|
| `Qwen/Qwen3-Embedding-4B` | Converts code → vectors for dense search | Index building |
| `Qwen/Qwen3-Reranker-4B` | Scores (query, code) pairs for precision | Search time |

Set via the interactive wizard on first run, or create manually. If embedding API is unavailable, graph-based search still works.

## Development

```bash
cd rust-core

# Build
cargo build

# Build release
cargo build --release

# Run tests
cargo test

# Run specific test
cargo test test_build_graph_functionality -- --nocapture
```

## License

MIT

Built with: Tree-sitter · Petgraph · LanceDB · Tantivy · Tokio · Clap · Axum