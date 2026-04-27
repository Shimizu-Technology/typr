#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="$ROOT_DIR/src-tauri/target/debug/bundle/macos/Typr.app"
IDENTIFIER="com.shimizutechnology.typr"

cd "$ROOT_DIR"
npm run tauri build -- --debug --bundles app

codesign --force --deep --sign - --identifier "$IDENTIFIER" "$APP_PATH"

echo
echo "Built and ad-hoc signed:"
echo "  $APP_PATH"
echo
echo "If Accessibility is enabled for an older Typr entry, remove it and add this app bundle path."
