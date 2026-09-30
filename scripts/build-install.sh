#!/bin/zsh
# Developer convenience; packages through the same verified release path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
(cd "$ROOT" && pnpm release:mac:dev)
APP="$ROOT/src-tauri/target/portable/aarch64-apple-darwin/release/bundle/macos/Belegal _ Local demo.app"
STAGE="$(mktemp -d /Applications/.belegal-local-demo-install.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
/usr/bin/ditto "$APP" "$STAGE/Belegal _ Local demo.app"
/usr/bin/codesign --verify --deep --strict "$STAGE/Belegal _ Local demo.app"
# Keep a rollback copy until installation succeeds.
if [ -d "/Applications/Belegal _ Local demo.app" ]; then
  mv "/Applications/Belegal _ Local demo.app" "$STAGE/previous.app"
fi
if ! mv "$STAGE/Belegal _ Local demo.app" "/Applications/Belegal _ Local demo.app"; then
  [ ! -d "$STAGE/previous.app" ] || mv "$STAGE/previous.app" "/Applications/Belegal _ Local demo.app"
  exit 1
fi
open "/Applications/Belegal _ Local demo.app"
