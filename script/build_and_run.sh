#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-dev}"
APP_NAME="PostdocOS"
PROCESS_NAME="postdocos"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="$ROOT_DIR/src-tauri/target/release/bundle/macos/$APP_NAME.app"
APP_BINARY="$APP_BUNDLE/Contents/MacOS/$PROCESS_NAME"
DMG_PATH="$ROOT_DIR/src-tauri/target/release/bundle/dmg/${APP_NAME}_0.1.0_aarch64.dmg"
INSTALLED_APP_BUNDLE="/Applications/$APP_NAME.app"
INSTALLED_APP_BINARY="$INSTALLED_APP_BUNDLE/Contents/MacOS/$PROCESS_NAME"
INSTALL_BACKUP_BUNDLE=""
FAILED_INSTALL_BUNDLE=""

stop_app() {
  pkill -x "$PROCESS_NAME" >/dev/null 2>&1 || true
}

run_checks() {
  npm run typecheck
  npm test -- --run
  (cd "$ROOT_DIR/src-tauri" && cargo test)
}

launch_app_bundle() {
  local bundle="$1"
  local binary="$bundle/Contents/MacOS/$PROCESS_NAME"
  if [[ ! -x "$binary" ]]; then
    echo "App binary is missing: $binary" >&2
    exit 1
  fi
  /usr/bin/open -n "$bundle"
}

verify_running_build() {
  local expected_binary="${1:-$APP_BINARY}"
  sleep 2
  local pid
  pid="$(pgrep -n -x "$PROCESS_NAME" || true)"
  if [[ -z "$pid" ]]; then
    echo "$APP_NAME did not start." >&2
    return 1
  fi
  local command
  command="$(ps -p "$pid" -o command=)"
  if [[ "$command" != "$expected_binary"* ]]; then
    echo "Wrong app is running: $command" >&2
    echo "Expected: $expected_binary" >&2
    return 1
  fi
  echo "Verified fresh build: $command"
  shasum -a 256 "$expected_binary"
}

install_unsigned_app() {
  if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "Built app is missing: $APP_BUNDLE" >&2
    exit 1
  fi

  local stamp
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  local backup_root="$ROOT_DIR/backups/installed-apps"
  local staging_dir
  staging_dir="$(mktemp -d /private/tmp/postdocos-install.XXXXXX)"
  local staged_bundle="$staging_dir/$APP_NAME.app"
  INSTALL_BACKUP_BUNDLE="$backup_root/${APP_NAME}-${stamp}.app"
  FAILED_INSTALL_BUNDLE="$backup_root/${APP_NAME}-${stamp}-failed.app"

  /usr/bin/ditto "$APP_BUNDLE" "$staged_bundle"
  mkdir -p "$backup_root"
  if [[ -d "$INSTALLED_APP_BUNDLE" ]]; then
    mv "$INSTALLED_APP_BUNDLE" "$INSTALL_BACKUP_BUNDLE"
  else
    INSTALL_BACKUP_BUNDLE=""
  fi

  if ! mv "$staged_bundle" "$INSTALLED_APP_BUNDLE"; then
    if [[ -n "$INSTALL_BACKUP_BUNDLE" && -d "$INSTALL_BACKUP_BUNDLE" ]]; then
      mv "$INSTALL_BACKUP_BUNDLE" "$INSTALLED_APP_BUNDLE"
    fi
    echo "Installation failed; the previous app was restored." >&2
    exit 1
  fi
  rmdir "$staging_dir"
}

restore_previous_install() {
  stop_app
  if [[ -d "$INSTALLED_APP_BUNDLE" ]]; then
    mv "$INSTALLED_APP_BUNDLE" "$FAILED_INSTALL_BUNDLE"
  fi
  if [[ -n "$INSTALL_BACKUP_BUNDLE" && -d "$INSTALL_BACKUP_BUNDLE" ]]; then
    mv "$INSTALL_BACKUP_BUNDLE" "$INSTALLED_APP_BUNDLE"
    echo "Fresh install failed to launch; the previous app was restored." >&2
  fi
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
    launch_app_bundle "$APP_BUNDLE"
    verify_running_build
    ;;
  --install-unsigned|install-unsigned)
    run_checks
    npm run desktop:build -- --bundles app --no-sign
    stop_app
    install_unsigned_app
    launch_app_bundle "$INSTALLED_APP_BUNDLE"
    if ! verify_running_build "$INSTALLED_APP_BINARY"; then
      restore_previous_install
      exit 1
    fi
    echo "Installed unsigned local build: $INSTALLED_APP_BUNDLE"
    if [[ -n "$INSTALL_BACKUP_BUNDLE" ]]; then
      echo "Previous installed app backup: $INSTALL_BACKUP_BUNDLE"
    fi
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
    echo "usage: $0 [dev|--verify|--install-unsigned|--release|--logs|--telemetry|--debug]" >&2
    exit 2
    ;;
esac
