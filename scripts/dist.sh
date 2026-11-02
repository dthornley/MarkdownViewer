#!/bin/bash
# Builds the macOS app and packages it into release/.
#
#   ./scripts/dist.sh --unsigned   ad-hoc signed .app and .dmg (fine for your own Mac)
#   ./scripts/dist.sh              Developer ID signed, notarized and stapled .app, .dmg and .zip
#
# Signing uses the "Developer ID Application" identity from the login keychain (or
# APPLE_SIGNING_IDENTITY). Notarization uses the notarytool keychain profile named by
# APPLE_KEYCHAIN_PROFILE (default: MarkdownViewer). One-time setup on a new Mac:
#   xcrun notarytool store-credentials "MarkdownViewer" --apple-id <apple-id> --team-id <team-id>
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"

PRODUCT="Markdown Viewer"
VERSION=$(node -p "require('./package.json').version")
ARCH=$(uname -m | sed 's/x86_64/x64/')
PROFILE="${APPLE_KEYCHAIN_PROFILE:-MarkdownViewer}"
BUNDLE_DIR="src-tauri/target/release/bundle"
APP="$BUNDLE_DIR/macos/$PRODUCT.app"
OUT="release"
DMG="$OUT/$PRODUCT-$VERSION-$ARCH.dmg"
ZIP="$OUT/$PRODUCT-$VERSION-$ARCH.zip"

rm -rf "$OUT"; mkdir -p "$OUT"

# Plain drag-to-install dmg via hdiutil. (Tauri's own dmg bundler styles the Finder window
# through AppleScript, which needs an Automation permission the terminal may not have.)
make_dmg() {
  local stage; stage=$(mktemp -d)
  cp -R "$APP" "$stage/"; ln -s /Applications "$stage/Applications"
  hdiutil create -volname "$PRODUCT" -srcfolder "$stage" -ov -format UDZO -quiet "$DMG"
  rm -rf "$stage"
}

if [[ "${1:-}" == "--unsigned" ]]; then
  echo "==> Building $PRODUCT $VERSION (ad-hoc signed)..."
  APPLE_SIGNING_IDENTITY="-" npx tauri build --bundles app
  cp -R "$APP" "$OUT/"
  make_dmg
  echo "==> Done:"; ls -1 "$OUT"
  exit 0
fi

IDENTITY="${APPLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning 2>/dev/null \
  | grep -o '"Developer ID Application: [^"]*"' | head -1 | tr -d '"')}"
[[ -n "$IDENTITY" ]] || { echo "No 'Developer ID Application' identity in the keychain. Use --unsigned or set APPLE_SIGNING_IDENTITY." >&2; exit 1; }
xcrun notarytool history --keychain-profile "$PROFILE" >/dev/null 2>&1 \
  || { echo "No notarytool keychain profile '$PROFILE'. Create it once with:" >&2
       echo "  xcrun notarytool store-credentials \"$PROFILE\" --apple-id <apple-id> --team-id <team-id>" >&2; exit 1; }

echo "==> Building $PRODUCT $VERSION, signing with: $IDENTITY"
APPLE_SIGNING_IDENTITY="$IDENTITY" npx tauri build --bundles app
codesign --verify --deep --strict "$APP"

echo "==> Notarizing the app..."
ditto -c -k --keepParent "$APP" "$ZIP"
xcrun notarytool submit "$ZIP" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$APP"
rm -f "$ZIP"; ditto -c -k --keepParent "$APP" "$ZIP"   # re-zip the stapled app
cp -R "$APP" "$OUT/"

echo "==> Building and notarizing the dmg..."
make_dmg
codesign --sign "$IDENTITY" --timestamp "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"

echo "==> Verifying..."
spctl --assess --type execute --verbose=2 "$OUT/$PRODUCT.app" 2>&1 | sed 's/^/   /'
xcrun stapler validate "$DMG" | sed 's/^/   /'
echo "==> Done:"; ls -1 "$OUT"
