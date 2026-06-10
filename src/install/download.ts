/**
 * CodeXray Binary Downloader
 *
 * Downloads the appropriate platform binary from GitHub Releases,
 * extracts the tar.gz archive, and installs the binary to ~/.codexray/bin/.
 * Can be called from npm postinstall or on first-run.
 */

import * as https from "https";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { execSync } from "child_process";

const REPO_OWNER = "iohub";
const REPO_NAME = "codexray";
// Read version from package.json — single source of truth
const VERSION = "v" + require("../../package.json").version;

function getPlatformSuffix(): string {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === "darwin") {
    if (arch === "arm64") return "darwin-arm64";
    return "darwin-x64";
  }
  if (platform === "linux") {
    return "linux-x64";
  }
  if (platform === "win32") {
    return "win32-x64";
  }
  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

function getDownloadUrl(): string {
  const suffix = getPlatformSuffix();
  return `https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${VERSION}/codexray-${suffix}.tar.gz`;
}

export function downloadBinary(destPath?: string): Promise<string> {
  const url = getDownloadUrl();
  console.log(`  Downloading from ${url}`);

  const dest = destPath || path.join(
    os.homedir(),
    ".codexray",
    "bin",
    process.platform === "win32" ? "codexray.exe" : "codexray"
  );

  // Ensure destination directory exists
  const destDir = path.dirname(dest);
  if (!fs.existsSync(destDir)) {
    fs.mkdirSync(destDir, { recursive: true });
  }

  // Download to a temp file first, then extract
  const tmpArchive = path.join(
    os.tmpdir(),
    `codexray-${Date.now()}.tar.gz`
  );

  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(tmpArchive);

    function doDownload(downloadUrl: string): void {
      https.get(downloadUrl, (response) => {
        // Follow redirects
        if (
          response.statusCode === 301 ||
          response.statusCode === 302 ||
          response.statusCode === 307
        ) {
          const redirectUrl = response.headers.location;
          if (!redirectUrl) {
            cleanup(reject, new Error("Redirect with no location"));
            return;
          }
          doDownload(redirectUrl);
          return;
        }

        if (response.statusCode !== 200) {
          cleanup(
            reject,
            new Error(`Download failed: HTTP ${response.statusCode}`)
          );
          return;
        }

        response.pipe(file);
        file.on("finish", () => {
          file.close();
          extractAndInstall();
        });
      }).on("error", (err) => {
        cleanup(reject, err);
      });
    }

    function extractAndInstall(): void {
      try {
        // Extract tar.gz to a temp directory
        const tmpExtractDir = path.join(
          os.tmpdir(),
          `codexray-extract-${Date.now()}`
        );
        fs.mkdirSync(tmpExtractDir, { recursive: true });

        execSync(`tar -xzf "${tmpArchive}" -C "${tmpExtractDir}"`, {
          stdio: "pipe",
        });

        // Archive structure: codexray-<suffix>/codexray
        const entries = fs.readdirSync(tmpExtractDir);
        const binDir = entries.find((e) => e.startsWith("codexray-"));
        if (!binDir) {
          throw new Error(
            `Could not find codexray directory in archive (found: ${entries.join(", ")})`
          );
        }

        const extractedBinary = path.join(tmpExtractDir, binDir, "codexray");
        if (!fs.existsSync(extractedBinary)) {
          throw new Error(`Binary not found in archive at ${binDir}/codexray`);
        }

        // Move binary to final destination
        fs.renameSync(extractedBinary, dest);

        // Cleanup temp files
        try {
          fs.unlinkSync(tmpArchive);
        } catch { /* ignore */ }
        try {
          fs.rmSync(tmpExtractDir, { recursive: true });
        } catch { /* ignore */ }

        resolve(dest);
      } catch (err) {
        cleanup(reject, err);
      }
    }

    function cleanup(rejectFn: (err: Error) => void, err: unknown): void {
      try {
        fs.unlinkSync(tmpArchive);
      } catch { /* ignore */ }
      rejectFn(err instanceof Error ? err : new Error(String(err)));
    }

    file.on("error", (err) => {
      try {
        fs.unlinkSync(tmpArchive);
      } catch { /* ignore */ }
      reject(err);
    });

    doDownload(url);
  });
}

// Auto-run when called directly (e.g. from postinstall)
if (require.main === module) {
  downloadBinary()
    .then((p) => {
      console.log(`  ✓ CodeXray binary installed to ${p}`);
      // Make it executable
      try {
        fs.chmodSync(p, 0o755);
      } catch { /* ignore on Windows */ }
    })
    .catch((err) => {
      console.error(`  ⚠ Binary download skipped: ${err.message}`);
      console.error(
        `  Run 'codexray' to download the binary on first use.`
      );
    });
}
