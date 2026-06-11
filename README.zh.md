# CodeXRay

**AI 时代的代码搜索与知识引擎。** 集成语义检索（稠密 + 稀疏 + RRF + 重排）、代码调用图、实时索引，一站式解决代码理解、知识管理和智能搜索问题。

> 专为 Claude Code / Codex CLI 原生构建——零守护进程、零配置负担。一条命令安装即可获得 6 个 MCP 工具。

## 亮点

### 🧠 混合检索引擎 — 语义 + 全文双通道

不止是关键词匹配。稠密向量检索理解代码意图（"登录逻辑"→ `authenticateUser`），BM25 全文索引锁定精确命中。经 RRF 融合 + Cross-Encoder 重排，结果精准可靠。Embedding API 不可用时自动降级到图搜索，永不中断。

### 🔗 四维知识图谱

代码调用图 + 代码向量 + Commit 向量 + 知识向量，四重维度构筑完整的代码库认知。Tree-sitter AST 解析 7 种语言，构建函数/类/方法的完整关系网：

- **谁调用了这个函数？**
- **这个函数依赖什么？**
- **一段描述就能搜到相关代码**

### ⚡ 实时增量索引

首次全量构建，之后仅重处理变更文件（MD5 对比）。三种触发模式：MCP 启动时、Git hook 提交时、文件监听守护进程。自动清理已删除文件的孤儿 Embedding，索引永不膨胀。

### 🔌 原生 MCP，本地化部署

专为 Claude Code/Codex CLI MCP stdio 协议构建。安装即注册，无需手动配置 MCP，无需常驻守护进程。随 Claude Code 启停，零残留。代码和数据全在本地，无需 SaaS。

### 🚀 一条命令安装

```bash
curl -fsSL https://raw.githubusercontent.com/iohub/codexray/main/install.sh | sh
```

自动识别 OS、架构、libc，一键完成下载、安装、MCP 注册。

## 快速开始

### 一键 curl 安装（推荐）

自动识别你的 OS、架构和 libc 变体：

```bash
curl -fsSL https://raw.githubusercontent.com/iohub/codexray/main/install.sh | sh
```

脚本执行完成后，重启 Claude Code——它会自动发现 codexray 的 MCP 工具。

> **首次运行：** 在终端中运行脚本时会进入交互式配置向导。如果通过管道运行（非交互模式），Graph 搜索开箱即用，之后可手动运行 `codexray install` 配置 Embedding API。

### 手动下载安装

各平台预编译二进制：

| 平台 | 命令 |
|------|------|
| **macOS (Apple Silicon)** | `curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-darwin-arm64.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |
| **macOS (Intel)** | `curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-darwin-x64.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |
| **Linux (glibc)** | `curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-linux-x64.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |
| **Linux (musl)** | `curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-linux-x64-musl.tar.gz && tar -xzf codexray.tar.gz && ./codexray install` |

执行后重启 Claude Code，它将自动发现以下 MCP 工具：`codexray_explore`、`codexray_search`、`codexray_find`、`codexray_callers`、`codexray_callees`、`codexray_status`。服务器在启动时自动索引项目，在 Claude Code 关闭时优雅退出——无需守护进程或手动命令。

## 安装方式

### 二进制（GitHub Releases）

每个版本提供预编译二进制：

| 平台 | 架构 | 下载文件 |
|------|------|----------|
| macOS | Apple Silicon (arm64) | `codexray-darwin-arm64.tar.gz` |
| macOS | Intel (x64) | `codexray-darwin-x64.tar.gz` |
| Linux | x64 (glibc) | `codexray-linux-x64.tar.gz` |
| Linux | x64 (musl, 静态) | `codexray-linux-x64-musl.tar.gz` |

从[最新发布](https://github.com/iohub/codexray/releases/latest)下载对应平台的压缩包，解压后运行 `codexray install`：

```bash
# macOS (Apple Silicon)
curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-darwin-arm64.tar.gz
tar -xzf codexray.tar.gz
./codexray install

# macOS (Intel)
curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-darwin-x64.tar.gz
tar -xzf codexray.tar.gz
./codexray install

# Linux (x64 glibc)
curl -L -o codexray.tar.gz https://github.com/iohub/codexray/releases/latest/download/codexray-linux-x64.tar.gz
tar -xzf codexray.tar.gz
./codexray install
```

`install` 命令将 codexray 注册为 Claude Code / Codex CLI 的 MCP 工具。首次运行时还会启动交互式配置向导，引导你配置 Embedding 模型和 API Token。

### 源码编译

```bash
git clone https://github.com/iohub/codexray.git
cd codexray
cargo build --release
```

编译后的二进制位于 `rust-core/target/release/codexray`，直接运行：

```bash
./rust-core/target/release/codexray install
```

首次运行时，`./codexray install` 也会启动交互式配置向导。

## 工作原理

### 索引构建

```
源文件
  → Tree-sitter AST 解析（7 种语言）
  → 提取函数 / 类 / 方法
  → 构建调用图（PetCodeGraph）
  → 批量 Embedding API 调用（SQLite 缓存）
  → 向量存入 LanceDB
  → 构建 BM25 索引到 Tantivy
  → 保存到 ~/.codexray/<project_hash>/
```

**幂等性**：索引构建是增量的——首次全量构建，后续运行通过 MD5 哈希对比，仅重处理变更文件。使用 `codexray install-hooks` 可在 git commit/merge 时自动重索引，或使用 `codexray daemon` 实现实时文件监听。

### 混合搜索管道

```
                        ┌─────────────────────┐
用户查询 ──────────────→│   Embedding 模型    │──→ 查询向量
                        └─────────────────────┘
                                  │
          ┌───────────────────────┼───────────────────────┐
          ▼                       ▼                       ▼
   ┌─────────────┐       ┌─────────────┐       ┌─────────────┐
   │ 稠密搜索     │       │ 稀疏搜索     │       │ 图搜索       │
   │ (LanceDB ANN)│       │ (Tantivy BM25)│      │(PetCodeGraph)│
   └──────┬───────┘       └──────┬──────┘       └──────┬──────┘
          │                      │                      │
          └──────────────────────┼──────────────────────┘
                                 ▼
                        ┌─────────────────┐
                        │   RRF 融合      │  ← Reciprocal Rank Fusion
                        │  (Top-20 候选)   │
                        └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │    重排器       │  ← Cross-Encoder 精排
                        │ (Qwen3-Reranker)│     逐对打分 (query, code)
                        └────────┬────────┘
                                 │
                                 ▼
                        ┌─────────────────┐
                        │   最终结果       │  ← Top-5（或 Top-N）
                        └─────────────────┘
```

| 阶段 | 技术 | 作用 |
|------|------|------|
| **稠密搜索** | LanceDB + Embedding 模型 | 语义向量相似度检索 |
| **稀疏搜索** | Tantivy BM25 | 关键词和 Token 匹配 |
| **RRF 融合** | Reciprocal Rank Fusion | 公平合并异源分数 |
| **重排器** | Cross-Encoder (Qwen3-Reranker-4B) | 全交互精排打分 |
| **降级** | PetCodeGraph | 基于名称的图搜索（无需 API） |

如果 Embedding 或 Reranker 不可用，管道会优雅降级到 Graph 搜索和 BM25-only 模式。

### 自动索引模式

| 模式 | 触发时机 | 命令 |
|------|----------|------|
| **Git hooks** | 提交/合并时 | `codexray install-hooks` |
| **守护进程** | 文件变更（8s 防抖） | `codexray daemon` |
| **MCP 服务器** | 启动时 + 文件变更 | `codexray install` + 重启 Claude Code |

MCP 服务器自动集成所有三种模式：启动时初始索引、运行时文件监听、以及 CLAUDE.md 注入实现工具发现。

### 存储

- **配置**: `~/.codexray/config.json`（全局，所有项目共享）
- **索引**: `~/.codexray/<md5(project_root)>/`
  - `project.json` — 项目元数据
  - `graph.bin` — 序列化调用图
  - `embeddings.lance/` — LanceDB 向量数据
  - `tantivy_bm25/` — BM25 全文索引
  - `file_hashes.json` — MD5 增量追踪
  - `embedding_hashes.json` — Embedding 增量追踪

无守护进程，无 HTTP 服务（除非运行 `daemon` 或 `serve --mcp`）。所有 CLI 命令均为独立进程。

## 支持的语言

| 语言 | 函数 | 结构体/类 | 调用图 |
|------|:----:|:---------:|:------:|
| Rust | ✅ | ✅ | ✅ |
| Python | ✅ | ✅ | ✅ |
| JavaScript | ✅ | ✅ | ✅ |
| TypeScript | ✅ | ✅ | ✅ |
| Go | ✅ | ✅ | ✅ |
| C/C++ | ✅ | ✅ | ✅ |
| Java | ✅ | ✅ | ✅ |

## 配置

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

### 模型角色

| 模型 | 作用 | 时机 |
|------|------|------|
| `Qwen/Qwen3-Embedding-4B` | 代码 → 向量，用于稠密搜索 | 索引构建 |
| `Qwen/Qwen3-Reranker-4B` | 对 (query, code) 精排打分 | 搜索时 |

首次运行时会通过交互式向导配置，也可手动创建。如果 Embedding API 不可用，Graph 搜索仍然可用。

## 开发

```bash
cd rust-core

# 构建
cargo build

# 发布构建
cargo build --release

# 运行测试
cargo test

# 运行指定测试
cargo test test_build_graph_functionality -- --nocapture
```

## 许可

MIT

技术栈：Tree-sitter · Petgraph · LanceDB · Tantivy · Tokio · Clap · Axum
