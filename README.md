# CodeXray

**Code search & knowledge engine for the AI era.** Semantic + full-text hybrid search, real-time indexing, call graph + code vectors + commit vectors + knowledge vectors — unified into one native MCP server.

> Built natively for Claude Code/Codex CLI — zero daemon, zero config overhead.

> 📖 [中文文档](README.zh.md)

## Highlights

### 🧠 Hybrid Search Engine — Semantic + Full-Text Dual Channel

Goes beyond keyword matching. Dense vector search understands code intent ("login logic" → `authenticateUser`), while BM25 full-text search locks in exact matches. Results are fused via RRF and re-ranked by Cross-Encoder for precision. When embedding API is unavailable, gracefully falls back to graph search — never breaks.

### 🔗 4D Knowledge Graph

Call graph + code vectors + commit vectors + knowledge vectors — four dimensions of codebase awareness. Tree-sitter AST parses 7 languages to build complete function/class/method relationships:

- **Who calls this function?**
- **What does this function depend on?**
- **Find code by describing what it does**

### ⚡ Real-time Incremental Indexing

Full build on first run, only re-processes changed files thereafter (MD5 diff). Auto-indexes on MCP startup and watches file changes during runtime. Auto-cleans orphaned embeddings — index never bloats.

### 🔌 Native MCP, Local-First

Built specifically for Claude Code/Codex CLI MCP stdio protocol. Install registers MCP automatically — no manual config, no persistent daemon. Starts and exits with Claude Code, zero residue. All code and data stay local, no SaaS required.

## Quick Start

### Recommended — one curl command

```bash
curl -fsSL https://raw.githubusercontent.com/iohub/codexray/main/install.sh | sh
```

Auto-detects OS/arch/libc, downloads, installs, and registers MCP. Restart Claude Code after — done.

**First run:** `codexray install` auto-launches an interactive setup wizard for the embedding API (graph search works without configuration). CodeXray works out of the box for call graph and name search.

### Manual download (Linux musl example)

```bash
curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-linux-x64-musl.tar.gz
tar -xzf codexray.tar.gz
./codexray install && rm codexray.tar.gz
```

Other platforms: replace `linux-x64-musl` with `darwin-arm64`, `darwin-x64`, or `linux-x64` from the [latest release](https://github.com/iohub/codexray/releases/latest).

### From source

```bash
git clone https://github.com/iohub/codexray.git && cd codexray
cargo build --release && ./rust-core/target/release/codexray install
```

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

**Idempotent**: index builds are incremental — the first run is a full build, subsequent runs compare MD5 hashes and only re-process changed files.

### Hybrid Search Pipeline (`CodeXray search`)

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
| **MCP server** | On startup + file changes | `codexray install` + restart Claude Code |

The MCP server automatically indexes on startup, watches file changes during runtime, and injects CLAUDE.md for tool discovery.

### Storage

- **Config**: `~/.codexray/config.json` (global, shared across all projects)
- **Index**: `~/.codexray/<md5(project_root)>/`
  - `project.json` — Project metadata
  - `graph.bin` — Serialized call graph
  - `embeddings.lance/` — LanceDB vector data
  - `tantivy_bm25/` — BM25 full-text index
  - `file_hashes.json` — MD5 incremental tracking
  - `embedding_hashes.json` — Embedding incremental tracking

No daemon, no HTTP server. Every CLI command is a standalone process.

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
