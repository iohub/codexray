#!/usr/bin/env node

/**
 * CodeXray CLI Entry Point
 *
 * Flow:
 * 1. Check ~/.codexray/config.json exists
 *    -> No: Run interactive setup wizard
 * 2. Check ~/.codexray/bin/codexray exists
 *    -> No: Download platform binary from GitHub Releases
 * 3. Pass-through args to Rust binary
 */

import * as fs from "fs";
import * as path from "path";
import * as os from "os";
import { spawnSync } from "child_process";
import { downloadBinary } from "../install/download";

const HOME = os.homedir();
const CODERAY_DIR = path.join(HOME, ".codexray");
const CONFIG_PATH = path.join(CODERAY_DIR, "config.json");
const BIN_DIR = path.join(CODERAY_DIR, "bin");
const BIN_NAME = process.platform === "win32" ? "codexray.exe" : "codexray";
const BIN_PATH = path.join(BIN_DIR, BIN_NAME);

function ensureDir(dir: string): void {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true });
  }
}

/**
 * Check if we're being launched as an MCP server (serve --mcp).
 * In MCP mode, stdout must contain only JSON-RPC messages, so all
 * status/diagnostic output must go to stderr.
 */
function isMcpMode(): boolean {
  const args = process.argv.slice(2);
  return args.includes("serve") && args.includes("--mcp");
}

function logStatus(msg: string): void {
  if (isMcpMode()) {
    console.error(msg);
  } else {
    console.log(msg);
  }
}

function configExists(): boolean {
  return fs.existsSync(CONFIG_PATH);
}

function binaryExists(): boolean {
  return fs.existsSync(BIN_PATH);
}

/**
 * Interactive setup wizard.
 */
async function runSetupWizard(): Promise<void> {
  const readline = await import("readline");
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  const question = (q: string): Promise<string> =>
    new Promise((resolve) => rl.question(q, resolve));

  // ── Welcome ──────────────────────────────────────────────
  console.log("\n  ╔══════════════════════════════════════════════╗");
  console.log(  "  ║        Welcome to CodeXray!                  ║");
  console.log(  "  ║  Code intelligence MCP for Claude Code       ║");
  console.log(  "  ╚══════════════════════════════════════════════╝");

  // ── Explain embedding model ──────────────────────────────
  console.log("\n  ── About the Embedding Model ──\n");
  console.log("  CodeXray uses an embedding model to convert your code into");
  console.log("  vectors for semantic search. This enables natural language");
  console.log('  queries like "find authentication middleware" across your');
  console.log("  entire codebase.\n");
  console.log("  ℹ  Basic features (call graph, name search) work WITHOUT");
  console.log("     an embedding model. You can skip this step and configure");
  console.log("     it later by re-running 'codexray'.\n");

  // ── Provider ────────────────────────────────────────────
  console.log("  ── Step 1: Choose a Provider ──\n");
  console.log("  Providers that offer compatible embedding APIs:");
  console.log("    • SiliconFlow  — https://siliconflow.cn  (recommended)");
  console.log("    • OpenAI       — https://platform.openai.com");
  console.log("    • Any OpenAI-compatible API\n");

  const apiBaseUrl =
    (await question("  API Base URL [https://api.siliconflow.cn/v1]: ")) ||
    "https://api.siliconflow.cn/v1";

  // ── Model ───────────────────────────────────────────────
  console.log("\n  ── Step 2: Choose a Model ──\n");
  console.log("  Recommended embedding models:");
  console.log("    • Qwen/Qwen3-Embedding-4B  (SiliconFlow, free tier available)");
  console.log("    • BAAI/bge-large-zh-v1.5   (Chinese-optimized)");
  console.log("    • text-embedding-3-small    (OpenAI)\n");

  const model =
    (await question("  Model name [Qwen/Qwen3-Embedding-4B]: ")) ||
    "Qwen/Qwen3-Embedding-4B";

  // ── API Token ───────────────────────────────────────────
  console.log("\n  ── Step 3: API Token ──\n");
  console.log("  How to get an API token from SiliconFlow (recommended):");
  console.log("    1. Visit https://cloud.siliconflow.cn/account/ak");
  console.log("    2. Register or log in");
  console.log("    3. Click 'Create API Key'");
  console.log("    4. Copy the key (starts with 'sk-')");
  console.log("");
  console.log("  For OpenAI:");
  console.log("    1. Visit https://platform.openai.com/api-keys");
  console.log("    2. Create a new secret key\n");

  const apiToken = await question("  API Token: ");
  if (!apiToken || apiToken.trim().length < 5) {
    console.log("\n  ⚠  No valid API token provided.");
    console.log("  Skipping embedding configuration for now.");
    console.log("  You can re-run 'codexray' anytime to configure it.\n");
    rl.close();
    process.exit(0);
  }

  // ── Save ────────────────────────────────────────────────
  const config = {
    embedding: {
      provider: "openai-compatible",
      model: model.trim(),
      api_token: apiToken.trim(),
      api_base_url: apiBaseUrl.trim(),
      dimensions: 2560,
    },
    index: {
      min_code_block_length: 16,
      enable_reranker: false,
      hybrid: {
        enable_bm25: true,
        bm25_top_k: 100,
        vector_top_k: 100,
        rrf_k: 60.0,
        rrf_top_k: 20,
        short_code_threshold: 30,
        short_code_penalty: 0.5,
      },
      reranker: {
        enabled: false,
        model: "BAAI/bge-reranker-v2-m3",
        api_token: "",
        api_base_url: "https://api.siliconflow.cn/v1",
        top_n: 10,
        candidate_multiplier: 5,
        timeout_secs: 30,
      },
    },
    installed_hooks: {},
  };

  ensureDir(path.dirname(CONFIG_PATH));
  fs.writeFileSync(CONFIG_PATH, JSON.stringify(config, null, 2));

  console.log(`\n  ✓ Configuration saved to ${CONFIG_PATH}`);
  console.log("  Next step: run 'codexray init' to build your first index!\n");

  rl.close();
}

async function main(): Promise<void> {
  const mcpMode = isMcpMode();

  // Step 1: Check config
  if (!configExists()) {
    if (mcpMode) {
      console.error("CodeXray config not found. Run 'codexray' first to set up.");
      process.exit(1);
    }
    logStatus("First time setup — configuring CodeXRay...");
    await runSetupWizard();
  }

  // Step 2: Check binary
  if (!binaryExists()) {
    if (mcpMode) {
      console.error("CodeXray binary not found. Run 'codexray' first to download it.");
      process.exit(1);
    }
    logStatus("Downloading CodeXray binary...");
    ensureDir(BIN_DIR);
    try {
      await downloadBinary(BIN_PATH);
    } catch (err: any) {
      console.error(`Failed to download binary: ${err.message}`);
      console.error("Please install manually: cargo install codexray");
      console.error("Or clone the repo and run: ./build.sh");
      process.exit(1);
    }
  }

  // Step 3: Make executable
  try {
    fs.chmodSync(BIN_PATH, 0o755);
  } catch {
    // ignore on Windows
  }

  // Step 4: Pass through to Rust binary
  const args = process.argv.slice(2);
  const result = spawnSync(BIN_PATH, args, {
    stdio: "inherit",
    env: process.env,
  });

  if (result.error) {
    console.error(`Failed to run codexray: ${result.error.message}`);
    process.exit(1);
  }

  process.exit(result.status ?? 0);
}

main().catch((err) => {
  console.error("Fatal error:", err);
  process.exit(1);
});
