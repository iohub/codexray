#!/bin/bash
# Bump version, commit, tag, and push — one command release.
# Usage: ./bump-version.sh 0.1.1

set -e

# Detect OS for sed in-place compatibility
if [[ "$OSTYPE" == "darwin"* ]]; then
    SED_INPLACE=(-i '')
else
    SED_INPLACE=(-i)
fi

if [ -z "$1" ]; then
    echo "Usage: $0 <new-version>"
    echo "Example: $0 0.1.1"
    exit 1
fi

NEW_VERSION="$1"
ROOT="$(cd "$(dirname "$0")" && pwd)"

echo "==> Bumping version to $NEW_VERSION..."

cd "$ROOT"

# 1. package.json
sed "${SED_INPLACE[@]}" "s/\"version\": \".*\"/\"version\": \"$NEW_VERSION\"/" package.json

# 2. rust-core/Cargo.toml
sed "${SED_INPLACE[@]}" "s/^version = \".*\"/version = \"$NEW_VERSION\"/" rust-core/Cargo.toml

# 3. package-lock.json
npm install --package-lock-only 2>/dev/null || true

# 4. Commit
git add package.json package-lock.json rust-core/Cargo.toml
git commit -m "chore: bump version to $NEW_VERSION" 2>/dev/null || true

# 5. Push & tag
echo ""
echo "==> Pushing and tagging v$NEW_VERSION..."
git push
git tag "v$NEW_VERSION"
git push origin "v$NEW_VERSION"

echo ""
echo "==> Done! v$NEW_VERSION released."
echo "    CI: https://github.com/iohub/codexray/actions"
