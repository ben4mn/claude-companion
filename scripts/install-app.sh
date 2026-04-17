#!/usr/bin/env bash
# Build Companion and install the .app into /Applications.
#
# First run does a full Rust release build (3–5 min); subsequent runs are
# incremental. If an existing /Applications/Companion.app is currently open,
# macOS will refuse to overwrite it — we quit the running copy first.

set -euo pipefail

cd "$(dirname "$0")/.."

echo "[install] building release bundle..."
npm run build

APP_SRC="src-tauri/target/release/bundle/macos/Companion.app"
APP_DST="/Applications/Companion.app"

if [[ ! -d "$APP_SRC" ]]; then
  echo "[install] ERROR: expected $APP_SRC to exist after build" >&2
  exit 1
fi

if pgrep -x "Companion" >/dev/null 2>&1; then
  echo "[install] quitting running Companion..."
  osascript -e 'tell application "Companion" to quit' >/dev/null 2>&1 || true
  sleep 1
fi

echo "[install] copying to $APP_DST..."
rm -rf "$APP_DST"
cp -R "$APP_SRC" "$APP_DST"

echo "[install] done. Launch from Spotlight or /Applications."
