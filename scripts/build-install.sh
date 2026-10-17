#!/bin/zsh
# Developer convenience; packages through the same verified release path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
(cd "$ROOT" && pnpm release:mac:dev)
APP="$ROOT/src-tauri/target/portable/aarch64-apple-darwin/release/bundle/macos/Local Legal.app"
STAGE="$(mktemp -d /Applications/.local-legal-install.XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT
/usr/bin/ditto "$APP" "$STAGE/Local Legal.app"
/usr/bin/codesign --verify --deep --strict "$STAGE/Local Legal.app"
# Keep a rollback copy until installation succeeds.
if [ -d "/Applications/Local Legal.app" ]; then
  mv "/Applications/Local Legal.app" "$STAGE/previous.app"
fi
if ! mv "$STAGE/Local Legal.app" "/Applications/Local Legal.app"; then
  [ ! -d "$STAGE/previous.app" ] || mv "$STAGE/previous.app" "/Applications/Local Legal.app"
  exit 1
fi
open "/Applications/Local Legal.app"
