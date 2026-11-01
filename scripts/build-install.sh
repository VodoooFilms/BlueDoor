#!/bin/zsh
# Developer convenience; packages through the same verified release path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
(cd "$ROOT" && pnpm release:mac:dev)
APP="$ROOT/src-tauri/target/portable/aarch64-apple-darwin/release/bundle/macos/Bluedoor.app"
STAGE="$(mktemp -d /Applications/.bluedoor-install.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
/usr/bin/ditto "$APP" "$STAGE/Bluedoor.app"
/usr/bin/codesign --verify --deep --strict "$STAGE/Bluedoor.app"
# Keep a rollback copy until installation succeeds.
if [ -d "/Applications/Bluedoor.app" ]; then
  mv "/Applications/Bluedoor.app" "$STAGE/previous.app"
fi
if ! mv "$STAGE/Bluedoor.app" "/Applications/Bluedoor.app"; then
  [ ! -d "$STAGE/previous.app" ] || mv "$STAGE/previous.app" "/Applications/Bluedoor.app"
  exit 1
fi
open "/Applications/Bluedoor.app"
