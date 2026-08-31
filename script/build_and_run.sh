#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-dev}"
APP_NAME="PostdocOS"
PROCESS_NAME="postdocos"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="$ROOT_DIR/src-tauri/target/release/bundle/macos/$APP_NAME.app"
APP_BINARY="$APP_BUNDLE/Contents/MacOS/$PROCESS_NAME"
DMG_PATH="$ROOT_DIR/src-tauri/target/release/bundle/dmg/${APP_NAME}_0.1.0_aarch64.dmg"

stop_app() {
  pkill -x "$PROCESS_NAME" >/dev/null 2>&1 || true
}

run_checks() {
  npm run typecheck
  npm test -- --run
  (cd "$ROOT_DIR/src-tauri" && cargo test)
}

launch_built_app() {
  if [[ ! -x "$APP_BINARY" ]]; then
    echo "Built app is missing: $APP_BINARY" >&2
    exit 1
  fi
  /usr/bin/open -n "$APP_BUNDLE"
}

verify_running_build() {
  sleep 2
  local pid
  pid="$(pgrep -n -x "$PROCESS_NAME" || true)"
  if [[ -z "$pid" ]]; then
    echo "$APP_NAME did not start." >&2
    exit 1
  fi
  local command
  command="$(ps -p "$pid" -o command=)"
  if [[ "$command" != "$APP_BINARY"* ]]; then
    echo "Wrong app is running: $command" >&2
    echo "Expected: $APP_BINARY" >&2
    exit 1
  fi
  echo "Verified fresh build: $command"
  shasum -a 256 "$APP_BINARY"
}

cd "$ROOT_DIR"

case "$MODE" in
  dev|run)
    stop_app
    echo "Starting Tauri development mode with Vite hot reload."
    exec npm run desktop:dev
    ;;
  --verify|verify)
    stop_app
    run_checks
    npm run desktop:build -- --bundles app
    launch_built_app
    verify_running_build
    ;;
  --release|release)
    stop_app
    run_checks
    npm run desktop:build
    echo "Release app: $APP_BUNDLE"
    echo "Release DMG: $DMG_PATH"
    ;;
  --logs|logs)
    stop_app
    npm run desktop:dev &
    /usr/bin/log stream --info --style compact --predicate "process == \"$PROCESS_NAME\""
    ;;
  --telemetry|telemetry)
    stop_app
    npm run desktop:dev &
    /usr/bin/log stream --info --style compact --predicate 'subsystem == "com.postdocos.desktop" OR process == "postdocos"'
    ;;
  --debug|debug)
    stop_app
    RUST_BACKTRACE=1 RUST_LOG=debug exec npm run desktop:dev
    ;;
  *)
    echo "usage: $0 [dev|--verify|--release|--logs|--telemetry|--debug]" >&2
    exit 2
    ;;
esac
