#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="$ROOT_DIR/src-tauri/target/debug/bundle/macos/Typr.app"
IDENTIFIER="com.shimizutechnology.typr"

cd "$ROOT_DIR"
npm run tauri build -- --debug --bundles app

if [[ -n "${TYPR_CODESIGN_IDENTITY:-}" ]]; then
  SIGN_IDENTITY="$TYPR_CODESIGN_IDENTITY"
else
  SIGN_IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | awk -F '"' '/"Apple Development:|"Developer ID Application:|"Mac Developer:/ { print $2; exit }' || true)"
fi

if [[ -z "$SIGN_IDENTITY" ]]; then
  SIGN_IDENTITY="-"
  echo "No Apple Development/Developer ID code-signing identity found; falling back to ad-hoc signing."
  echo "Accessibility permissions may not persist across rebuilds while using ad-hoc signing."
fi

codesign --force --deep --sign "$SIGN_IDENTITY" --identifier "$IDENTIFIER" "$APP_PATH"

echo
echo "Built and signed:"
echo "  $APP_PATH"
echo "Signing identity:"
echo "  $SIGN_IDENTITY"
echo
echo "Code signature:"
codesign -dv --verbose=2 "$APP_PATH" 2>&1 | sed 's/^/  /'
echo
echo "If Accessibility is enabled for an older Typr entry, remove it and add this app bundle path."
