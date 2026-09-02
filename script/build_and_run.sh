#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-dev}"
APP_NAME="PostdocOS"
PROCESS_NAME="postdocos"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_VERSION="$(node -e 'const fs = require("fs"); console.log(JSON.parse(fs.readFileSync(process.argv[1], "utf8")).version)' "$ROOT_DIR/src-tauri/tauri.conf.json")"
APP_BUNDLE="$ROOT_DIR/src-tauri/target/release/bundle/macos/$APP_NAME.app"
APP_BINARY="$APP_BUNDLE/Contents/MacOS/$PROCESS_NAME"
case "$(uname -m)" in
  arm64) DMG_ARCH="aarch64" ;;
  x86_64) DMG_ARCH="x64" ;;
  *)
    echo "Unsupported macOS architecture: $(uname -m)" >&2
    exit 1
    ;;
esac
DMG_PATH="$ROOT_DIR/src-tauri/target/release/bundle/dmg/${APP_NAME}_${APP_VERSION}_${DMG_ARCH}.dmg"
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

adhoc_sign_app() {
  if [[ ! -d "$APP_BUNDLE" ]]; then
    echo "Built app is missing: $APP_BUNDLE" >&2
    exit 1
  fi

  local executable
  while IFS= read -r -d '' executable; do
    if [[ "$executable" == "$APP_BINARY" ]]; then
      continue
    fi
    if [[ "$(/usr/bin/file -b "$executable")" == Mach-O* ]]; then
      codesign --force --sign - --timestamp=none --options runtime "$executable"
    fi
  done < <(find "$APP_BUNDLE/Contents" -type f -perm -111 -print0)

  codesign --force --sign - --timestamp=none --options runtime "$APP_BUNDLE"
  codesign --verify --deep --strict --verbose=4 "$APP_BUNDLE"
}

create_adhoc_dmg() (
  local staging_dir
  staging_dir="$(mktemp -d /private/tmp/postdocos-adhoc-dmg.XXXXXX)"
  trap 'rm -rf "$staging_dir"' EXIT

  /usr/bin/ditto "$APP_BUNDLE" "$staging_dir/$APP_NAME.app"
  ln -s /Applications "$staging_dir/Applications"
  mkdir -p "$(dirname "$DMG_PATH")"
  hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$staging_dir" \
    -ov \
    -format UDZO \
    "$DMG_PATH"
  codesign --force --sign - --timestamp=none "$DMG_PATH"
)

verify_adhoc_dmg() (
  local mount_dir
  local mounted=0
  mount_dir="$(mktemp -d /private/tmp/postdocos-adhoc-verify.XXXXXX)"
  cleanup() {
    if [[ "$mounted" -eq 1 ]]; then
      hdiutil detach "$mount_dir" >/dev/null
    fi
    rmdir "$mount_dir"
  }
  trap cleanup EXIT

  codesign --verify --strict --verbose=4 "$DMG_PATH"
  hdiutil verify "$DMG_PATH"
  hdiutil attach -nobrowse -readonly -mountpoint "$mount_dir" "$DMG_PATH" >/dev/null
  mounted=1
  codesign --verify --deep --strict --verbose=4 "$mount_dir/$APP_NAME.app"
)

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
  --adhoc-dmg|adhoc-dmg)
    npm run desktop:build -- --bundles app --no-sign
    adhoc_sign_app
    create_adhoc_dmg
    verify_adhoc_dmg
    echo "Ad-hoc signed app: $APP_BUNDLE"
    echo "Ad-hoc signed DMG: $DMG_PATH"
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
    echo "usage: $0 [dev|--verify|--install-unsigned|--release|--adhoc-dmg|--logs|--telemetry|--debug]" >&2
    exit 2
    ;;
esac
