#!/bin/zsh
# Compatibility entry point for a local ad-hoc DMG. Commercial releases use
# pnpm release:mac and require Developer ID plus notarytool credentials.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export BLUEDOOR_RELEASE_MODE=development
exec python3 "$ROOT/scripts/release-mac.py" "$@"
