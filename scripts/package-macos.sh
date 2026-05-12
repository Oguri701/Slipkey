#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

APP_NAME="Slipkey"
BUNDLE_ID="dev.zlb.imeswitch"
VERSION="${VERSION:-0.1.0}"
SWIFT_SCRATCH="$ROOT/target/slipkey-swift"
MODULE_CACHE="$ROOT/target/swift-module-cache"
BUNDLE_DIR="${TMPDIR:-/tmp}/slipkey-package/bundle/macos"
APP_PATH="$BUNDLE_DIR/$APP_NAME.app"
DIST_DIR="$ROOT/dist"
ZIP_PATH="$DIST_DIR/$APP_NAME-$VERSION-macos-arm64.zip"

echo "==> Testing Rust workspace"
cargo test --workspace

# Note: imeswitchd is no longer bundled. The daemon survives only as a
# standalone CLI for diagnostics — `cargo build --release -p imeswitchd`
# if you need it. The macOS hook now runs in the Slipkey app's main
# process, so a single Accessibility grant covers everything.

echo "==> Testing Swift package"
swift test \
  --package-path "$ROOT/bins/slipkey-app" \
  --scratch-path "$SWIFT_SCRATCH"

echo "==> Building native app shell"
mkdir -p "$MODULE_CACHE"
CLANG_MODULE_CACHE_PATH="$MODULE_CACHE" \
  swift build \
    -c release \
    --package-path "$ROOT/bins/slipkey-app" \
    --scratch-path "$SWIFT_SCRATCH"

echo "==> Assembling $APP_NAME.app"
rm -rf "$APP_PATH"
mkdir -p "$APP_PATH/Contents/MacOS" "$APP_PATH/Contents/Resources" "$DIST_DIR"

cp "$ROOT/bins/slipkey-app/Info.plist" "$APP_PATH/Contents/Info.plist"
cp "$SWIFT_SCRATCH/release/$APP_NAME" "$APP_PATH/Contents/MacOS/$APP_NAME"
cp "$ROOT/bins/slipkey-app/Resources/icon.icns" "$APP_PATH/Contents/Resources/icon.icns"
cp "$ROOT/bins/slipkey-app/Resources/wechat-support.jpeg" "$APP_PATH/Contents/Resources/wechat-support.jpeg"

chmod +x "$APP_PATH/Contents/MacOS/$APP_NAME"
xattr -cr "$APP_PATH" || true
xattr -c "$APP_PATH" || true
xattr -d com.apple.FinderInfo "$APP_PATH" 2>/dev/null || true
/usr/libexec/PlistBuddy -c "Set :CFBundleExecutable $APP_NAME" "$APP_PATH/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleIdentifier $BUNDLE_ID" "$APP_PATH/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $VERSION" "$APP_PATH/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $VERSION" "$APP_PATH/Contents/Info.plist"
xattr -cr "$APP_PATH" || true
xattr -c "$APP_PATH" || true
xattr -d com.apple.FinderInfo "$APP_PATH" 2>/dev/null || true
rm -rf "$APP_PATH/Contents/_CodeSignature"

echo "==> Ad-hoc signing"
# No nested binaries to deep-sign — Resources/ holds only an icon.
codesign --force --sign - "$APP_PATH"
codesign --verify --deep --strict --verbose=2 "$APP_PATH" || \
  echo "  (warn: --deep --strict failed at $APP_PATH — continuing to /Applications re-sign)"

echo "==> Creating zip"
rm -f "$ZIP_PATH"
ditto -c -k --keepParent --noextattr --noqtn --norsrc "$APP_PATH" "$ZIP_PATH"
xattr -c "$ZIP_PATH" || true

# The source tree may live under iCloud Drive's "Desktop & Documents" sync,
# where fileprovider can continually re-add signing-hostile xattrs. Assemble
# and sign in TMPDIR, then install the final bundle to /Applications.
# /Applications is admin-writable on a default macOS install — this step does
# NOT require sudo for an admin user.
INSTALL_PATH="/Applications/$APP_NAME.app"
echo "==> Installing to $INSTALL_PATH"
pkill -x "$APP_NAME" 2>/dev/null || true
sleep 1
rm -rf "$INSTALL_PATH"
ditto --noextattr --noqtn --norsrc "$APP_PATH" "$INSTALL_PATH"
xattr -cr "$INSTALL_PATH"
codesign --force --sign - "$INSTALL_PATH"
codesign --verify --deep --strict --verbose=2 "$INSTALL_PATH" || \
  echo "  (warn: --deep --strict failed at $INSTALL_PATH — TCC may need re-grant)"

echo ""
echo "macOS build output:"
echo "  $APP_PATH       (signed staging artifact)"
echo "  $ZIP_PATH       (distributable)"
echo "  $INSTALL_PATH   (live install, run from here)"
echo ""
echo "If you'd previously granted Accessibility, the binary hash just changed"
echo "and TCC may silently deny. To re-grant after rebuild:"
echo "  tccutil reset Accessibility $BUNDLE_ID"
echo "  open $INSTALL_PATH"
echo "  # then toggle Slipkey on in System Settings → Privacy & Security → Accessibility"
