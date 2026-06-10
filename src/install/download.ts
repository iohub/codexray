/**
 * CodeXray Binary Downloader
 *
 * Downloads the appropriate platform binary from GitHub Releases.
 * Can be called from npm postinstall or on first-run.
 */

import * as https from "https";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

const REPO_OWNER = "iohub";
const REPO_NAME = "codexray";
// Read version from package.json — single source of truth
const VERSION = "v" + require("../../package.json").version;

function getPlatformSuffix(): { suffix: string; exe: boolean } {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === "darwin") {
    if (arch === "arm64") {
      return { suffix: "darwin-arm64", exe: false };
    }
    return { suffix: "darwin-x64", exe: false };
  }
  if (platform === "linux") {
    return { suffix: "linux-x64", exe: false };
  }
  if (platform === "win32") {
    return { suffix: "win32-x64", exe: true };
  }
  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

function getDownloadUrl(): string {
  const { suffix, exe } = getPlatformSuffix();
  const ext = exe ? ".exe" : "";
  return `https://github.com/${REPO_OWNER}/${REPO_NAME}/releases/download/${VERSION}/codexray-${suffix}${ext}`;
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

  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    const request = https.get(url, (response) => {
      // Follow redirects
      if (
        response.statusCode === 301 ||
        response.statusCode === 302 ||
        response.statusCode === 307
      ) {
        const redirectUrl = response.headers.location;
        if (!redirectUrl) {
          reject(new Error("Redirect with no location"));
          return;
        }
        https.get(redirectUrl, (redirectResponse) => {
          redirectResponse.pipe(file);
          file.on("finish", () => {
            file.close();
            resolve(dest);
          });
        }).on("error", reject);
        return;
      }

      if (response.statusCode !== 200) {
        reject(
          new Error(
            `Download failed: HTTP ${response.statusCode}`
          )
        );
        return;
      }

      response.pipe(file);
      file.on("finish", () => {
        file.close();
        resolve(dest);
      });
    });

    request.on("error", reject);
    file.on("error", reject);
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
      console.error(`  Run 'codexray' to download the binary on first use.`);
    });
}
