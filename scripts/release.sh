#!/bin/bash
# Builds a signed + notarized release and publishes it to GitHub Releases, including the
# files the in-app updater reads (the signed .app.tar.gz and latest.json).
#
#   ./scripts/release.sh            uses the version in package.json
#   ./scripts/release.sh 1.1.0      bumps package.json and Cargo.toml to 1.1.0 first
#
# Needs: the Apple signing setup from scripts/dist.sh, `gh` logged in, and the updater private
# key at TAURI_SIGNING_PRIVATE_KEY_PATH (default ~/.tauri/markdownviewer.key). Create the key once
# with `npx tauri signer generate -w ~/.tauri/markdownviewer.key` and put the matching .pub in
# src-tauri/tauri.conf.json (plugins.updater.pubkey). Back the key up: without it no future
# release can be installed by the updater.
# Release notes come from RELEASE_NOTES.md if present, otherwise from the commits since the last tag.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"

KEY="${TAURI_SIGNING_PRIVATE_KEY_PATH:-$HOME/.tauri/markdownviewer.key}"
[[ -f "$KEY" ]] || { echo "updater signing key not found at $KEY (see the comment at the top of this script)" >&2; exit 1; }
gh auth status >/dev/null 2>&1 || { echo "gh is not logged in" >&2; exit 1; }
REPO=$(gh repo view --json nameWithOwner --jq .nameWithOwner)

if [[ -n "${1:-}" ]]; then
  npm version "$1" --no-git-tag-version >/dev/null
  sed -i '' -E "0,/^version = \".*\"/s//version = \"$1\"/" src-tauri/Cargo.toml
  (cd src-tauri && cargo update -q -p markdown-viewer)   # keep Cargo.lock in step
fi
VERSION=$(node -p "require('./package.json').version")
TAG="v$VERSION"
ARCH=$(uname -m | sed 's/x86_64/x64/')
case "$ARCH" in arm64) TARGET=darwin-aarch64 ;; x64) TARGET=darwin-x86_64 ;; *) echo "unknown arch $ARCH" >&2; exit 1 ;; esac
export TARGET   # read by the latest.json step below

if git rev-parse "$TAG" >/dev/null 2>&1; then echo "tag $TAG already exists" >&2; exit 1; fi
if [[ -n "$(git status --porcelain)" ]]; then git add -A; git commit -q -m "v$VERSION"; fi

echo "==> Building $TAG (signed + notarized)..."
npm run dist --silent

APP="release/Markdown Viewer.app"
DMG="release/Markdown-Viewer-$VERSION-$ARCH.dmg"
ZIP="release/Markdown-Viewer-$VERSION-$ARCH.zip"
TAR="release/Markdown-Viewer-$VERSION-$ARCH.app.tar.gz"
for f in "$APP" "$DMG" "$ZIP"; do [[ -e "$f" ]] || { echo "missing $f" >&2; exit 1; }; done

echo "==> Signing the update archive..."
# The archive must hold the notarized, stapled app, so it is made here rather than by `tauri build`.
tar -czf "$TAR" -C release "Markdown Viewer.app"
npx tauri signer sign -f "$KEY" -p "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" "$TAR" >/dev/null
[[ -f "$TAR.sig" ]] || { echo "signing produced no $TAR.sig" >&2; exit 1; }

if [[ -f RELEASE_NOTES.md ]]; then
  NOTES=$(cat RELEASE_NOTES.md)
else
  LAST=$(git describe --tags --abbrev=0 2>/dev/null || true)
  NOTES=$(git log --pretty='- %s' ${LAST:+$LAST..}HEAD | grep -v '^- v[0-9]' || true)
fi

node -e '
const [version, notes, sig, url] = process.argv.slice(1);
const latest = { version, notes, pub_date: new Date().toISOString(), platforms: { [process.env.TARGET]: { signature: require("fs").readFileSync(sig, "utf8").trim(), url } } };
require("fs").writeFileSync("release/latest.json", JSON.stringify(latest, null, 2) + "\n");
' "$VERSION" "$NOTES" "$TAR.sig" "https://github.com/$REPO/releases/download/$TAG/$(basename "$TAR")"

echo "==> Tagging and pushing..."
git tag -a "$TAG" -m "$TAG"
git push -q origin HEAD
git push -q origin "$TAG"

echo "==> Creating GitHub release $TAG..."
gh release create "$TAG" "$DMG" "$ZIP" "$TAR" "$TAR.sig" release/latest.json --title "Markdown Viewer $VERSION" --notes "$NOTES"
echo "==> Done: $(gh release view "$TAG" --json url --jq .url)"
