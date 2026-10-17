#!/bin/zsh
# Developer convenience; packages through the same verified release path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
(cd "$ROOT" && pnpm release:mac:dev)
APP="$ROOT/src-tauri/target/portable/aarch64-apple-darwin/release/bundle/macos/BlueDoor.app"
STAGE="$(mktemp -d /Applications/.bluedoor-install.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
/usr/bin/ditto "$APP" "$STAGE/BlueDoor.app"
/usr/bin/codesign --verify --deep --strict "$STAGE/BlueDoor.app"
# Keep a rollback copy until installation succeeds.
if [ -d "/Applications/BlueDoor.app" ]; then
  mv "/Applications/BlueDoor.app" "$STAGE/previous.app"
fi
if ! mv "$STAGE/BlueDoor.app" "/Applications/BlueDoor.app"; then
  [ ! -d "$STAGE/previous.app" ] || mv "$STAGE/previous.app" "/Applications/BlueDoor.app"
  exit 1
fi
open "/Applications/BlueDoor.app"
