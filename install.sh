#!/bin/sh
#
# CodeXRay — one-curl install script
# Usage: curl -fsSL https://raw.githubusercontent.com/iohub/codexray/main/install.sh | sh
#
# Auto-detects OS, architecture, and libc variant, then downloads
# the correct pre-built binary from GitHub Releases and runs `codexray install`.
#
# Environment variables:
#   CODEXRAY_VERSION  — specify a version tag (default: latest)
#   CODEXRAY_DIR      — install directory (default: ~/.codexray)
#   CODEXRAY_DRY_RUN  — if set, print what would be done without downloading

set -e

# ── Constants ────────────────────────────────────────────────────────────
REPO="iohub/codexray"
BIN_NAME="codexray"
GITHUB="https://github.com/${REPO}/releases"

# ── Colors ───────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    CYAN='\033[0;36m'
    NC='\033[0m' # No Color
else
    RED=''; GREEN=''; YELLOW=''; CYAN=''; NC=''
fi

info()  { printf "${GREEN}✓${NC} %s\n" "$*"; }
warn()  { printf "${YELLOW}⚠${NC} %s\n" "$*"; }
error() { printf "${RED}✗${NC} %s\n" "$*"; }
header(){ printf "\n${CYAN}═══ %s ═══${NC}\n" "$*"; }

# ── Cleanup handler ──────────────────────────────────────────────────────
TMPDIR=""
cleanup() {
    exit_code=$?
    if [ -n "$TMPDIR" ] && [ -d "$TMPDIR" ]; then
        rm -rf "$TMPDIR"
    fi
    if [ "$exit_code" -ne 0 ]; then
        printf "\n"
        error "Installation failed (exit code: ${exit_code})."
        printf "  For help, open an issue at: https://github.com/${REPO}/issues\n"
    fi
    exit "$exit_code"
}
trap cleanup EXIT INT TERM

# ── Platform detection ───────────────────────────────────────────────────
detect_platform() {
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    ARCH="$(uname -m)"

    case "$OS" in
        darwin)  OS="darwin"  ;;
        linux)   OS="linux"   ;;
        *)
            error "Unsupported OS: ${OS} (expected darwin or linux)"
            exit 1
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64) ARCH="x64"  ;;
        aarch64|arm64) ARCH="arm64" ;;
        *)
            error "Unsupported architecture: ${ARCH} (expected x86_64 or aarch64)"
            exit 1
            ;;
    esac

    # Linux libc detection (glibc vs musl)
    LIBC="glibc"
    if [ "$OS" = "linux" ]; then
        # Detect musl via ldd
        if ldd --version 2>/dev/null | grep -qi musl; then
            LIBC="musl"
        fi
        # Additional detection for static-musl systems
        if [ -f /lib/ld-musl-x86_64.so.1 ] || [ -f /lib/ld-musl-aarch64.so.1 ]; then
            LIBC="musl"
        fi
    fi

    # Construct archive name
    if [ "$OS" = "darwin" ] && [ "$ARCH" = "arm64" ]; then
        ARCHIVE="${BIN_NAME}-darwin-arm64.tar.gz"
    elif [ "$OS" = "darwin" ] && [ "$ARCH" = "x64" ]; then
        ARCHIVE="${BIN_NAME}-darwin-x64.tar.gz"
    elif [ "$OS" = "linux" ] && [ "$ARCH" = "x64" ] && [ "$LIBC" = "musl" ]; then
        ARCHIVE="${BIN_NAME}-linux-x64-musl.tar.gz"
    elif [ "$OS" = "linux" ] && [ "$ARCH" = "x64" ]; then
        ARCHIVE="${BIN_NAME}-linux-x64.tar.gz"
    else
        error "No pre-built binary for ${OS}/${ARCH}/${LIBC}"
        printf "  Build from source: https://github.com/${REPO}#from-source\n"
        exit 1
    fi

    # Determine download URL
    if [ -n "${CODEXRAY_VERSION}" ]; then
        DOWNLOAD_URL="${GITHUB}/download/${CODEXRAY_VERSION}/${ARCHIVE}"
    else
        DOWNLOAD_URL="${GITHUB}/latest/download/${ARCHIVE}"
    fi

    # Checksum URL (only for tagged versions; latest redirect complicates checksums)
    if [ -n "${CODEXRAY_VERSION}" ]; then
        CHECKSUM_URL="${GITHUB}/download/${CODEXRAY_VERSION}/checksums.txt"
    else
        CHECKSUM_URL=""
    fi
}

# ── Dependency checks ────────────────────────────────────────────────────
check_deps() {
    if command -v curl >/dev/null 2>&1; then
        DOWNLOADER="curl"
    elif command -v wget >/dev/null 2>&1; then
        DOWNLOADER="wget"
    else
        error "Neither 'curl' nor 'wget' found. Install one of them and retry."
        exit 1
    fi

    for cmd in tar uname; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            error "Required command not found: ${cmd}"
            exit 1
        fi
    done
}

# ── Download helpers ─────────────────────────────────────────────────────
download() {
    url="$1"
    output="$2"
    desc="$3"

    info "Downloading ${desc}..."
    printf "  From: %s\n" "$url"

    case "${DOWNLOADER}" in
        curl)
            curl -fsSL --retry 3 --connect-timeout 10 "$url" -o "$output"
            ;;
        wget)
            wget -q --retry-connrefused --timeout=10 "$url" -O "$output"
            ;;
    esac

    if [ ! -f "$output" ] || [ ! -s "$output" ]; then
        error "Download failed: ${desc}"
        printf "  URL: %s\n" "$url"
        exit 1
    fi
}

verify_checksum() {
    archive="$1"
    checksum_url="$2"
    expected_file="${archive}.sha256"

    if [ -z "$checksum_url" ]; then
        return 0
    fi

    info "Verifying checksum..."
    if command -v sha256sum >/dev/null 2>&1; then
        CHECK_CMD="sha256sum"
    elif command -v shasum >/dev/null 2>&1; then
        CHECK_CMD="shasum -a 256"
    else
        warn "No sha256sum/shasum found; skipping checksum verification"
        return 0
    fi

    download "$checksum_url" "$expected_file" "checksums.txt"
    archive_name="$(basename "$archive")"
    if grep -q "${archive_name}" "$expected_file"; then
        if $CHECK_CMD -c "$expected_file" 2>/dev/null | grep -q "${archive_name}.*OK"; then
            info "Checksum verified"
            rm -f "$expected_file"
        else
            error "Checksum mismatch! File may be corrupted."
            rm -f "$expected_file"
            exit 1
        fi
    else
        warn "No checksum found for ${archive_name}; skipping verification"
        rm -f "$expected_file"
    fi
}

# ── Main install logic ───────────────────────────────────────────────────
main() {
    printf "${CYAN}"
    printf "  ╔══════════════════════════════════════════╗\n"
    printf "  ║          CodeXRay Installer              ║\n"
    printf "  ║  Code intelligence MCP server for Claude  ║\n"
    printf "  ╚══════════════════════════════════════════╝${NC}\n"

    detect_platform
    check_deps

    printf "\n  Platform: %s %s" "${OS}" "${ARCH}"
    if [ "${OS}" = "linux" ]; then
        printf " (%s)" "${LIBC}"
    fi
    printf "\n"

    if [ -n "${CODEXRAY_VERSION}" ]; then
        printf "  Version:  %s\n" "${CODEXRAY_VERSION}"
    else
        printf "  Version:  latest\n"
    fi
    printf "\n"

    # Dry run
    if [ -n "${CODEXRAY_DRY_RUN}" ]; then
        info "DRY RUN: would download ${DOWNLOAD_URL}"
        info "DRY RUN: would extract and run '${BIN_NAME} install'"
        exit 0
    fi

    # Create temp directory
    TMPDIR="$(mktemp -d "/tmp/${BIN_NAME}-install-XXXXXX")"
    ARCHIVE_PATH="${TMPDIR}/${ARCHIVE}"

    # Download archive
    download "$DOWNLOAD_URL" "$ARCHIVE_PATH" "${ARCHIVE}"

    # Verify checksum if available
    verify_checksum "$ARCHIVE_PATH" "$CHECKSUM_URL"

    # Extract archive
    info "Extracting archive..."
    tar -xzf "$ARCHIVE_PATH" -C "$TMPDIR"
    EXTRACTED_BIN="${TMPDIR}/${BIN_NAME}"
    if [ ! -f "$EXTRACTED_BIN" ]; then
        error "Binary not found in archive (expected '${BIN_NAME}')"
        printf "  Archive contents:\n"
        tar -tzf "$ARCHIVE_PATH" | sed 's/^/    /'
        exit 1
    fi
    chmod +x "$EXTRACTED_BIN"

    # Verify binary is executable
    if ! "$EXTRACTED_BIN" --help >/dev/null 2>&1; then
        warn "Binary may not be compatible with this system."
        printf "  Try building from source: https://github.com/${REPO}#from-source\n"
        exit 1
    fi

    info "Binary extracted and verified"

    # Run codexray install (copies binary to ~/.codexray/bin/ + MCP registration)
    header "Setup"
    printf "  Running '${BIN_NAME} install' to complete setup...\n\n"

    if [ -t 0 ]; then
        # Interactive terminal: pass stdin through
        "$EXTRACTED_BIN" install
    else
        # Non-interactive (pipe): skip wizard, use defaults
        info "Non-interactive mode detected."
        printf "  On first run, run '${BIN_NAME} install' interactively to configure your embedding API.\n"
        printf "  Graph-based search will work without configuration.\n\n"
        "$EXTRACTED_BIN" install --non-interactive 2>/dev/null || \
        "$EXTRACTED_BIN" install </dev/null 2>/dev/null || true
    fi

    # ── Add to PATH reminder ─────────────────────────────────────────
    BIN_DIR="${CODEXRAY_DIR:-$HOME/.codexray}/bin"
    header "Next Steps"

    case "$SHELL" in
        */zsh)  PROFILE_FILE="${HOME}/.zshrc" ;;
        */bash) PROFILE_FILE="${HOME}/.bashrc" ;;
        *)      PROFILE_FILE="" ;;
    esac

    case ":${PATH}:" in
        *:"${BIN_DIR}":*)
            info "${BIN_NAME} is in PATH"
            ;;
        *)
            warn "${BIN_DIR} is not in your PATH"
            printf "  Add this line to your shell profile"
            if [ -n "$PROFILE_FILE" ]; then
                printf " (${PROFILE_FILE})"
            fi
            printf ":\n\n"
            printf "    export PATH=\"\${PATH}:${BIN_DIR}\"\n\n"
            ;;
    esac

    printf "  Restart Claude Code — it will auto-discover codexray MCP tools.\n"
    printf "\n"
    printf "  ${GREEN}Installation complete!${NC}\n"
    printf "\n"
}

main "$@"
