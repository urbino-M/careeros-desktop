# Desktop Updates Handoff

## Scope

This subsystem checks the public CareerOS GitHub Release channel, presents release notes, downloads signed updater bundles, and asks the operating system installer to apply them. It owns the skipped-version and last-check preferences. It does not migrate application data, execute unsigned assets, or force an update without a user action.

## Architecture

`UpdateManager` calls Tauri's official updater plugin. The plugin fetches `latest.json`, compares versions, chooses the current platform artifact, and verifies its minisign signature against the public key embedded in `tauri.conf.json`. The release workflow creates both platform updater artifacts and publishes their signatures and URLs in `latest.json`. See Source Ownership in `docs/ARCHITECTURE.md`.

## Source Ownership

| Area | Primary files | Responsibility |
|---|---|---|
| Entry point | `src/updates/UpdateManager.tsx`; update card in `src/pages/SettingsPage.tsx` | Startup/hourly/manual checks, prompt state, progress, skip/retry actions |
| Native boundary | `src-tauri/src/lib.rs`; `src-tauri/capabilities/default.json`; `src-tauri/tauri.conf.json` | Register updater/process plugins, grant narrow permissions, pin endpoint and public key |
| Release integration | `src-tauri/tauri.updater.conf.json`; `.github/workflows/release.yml` | Enable updater artifacts only for release builds, sign them, publish `latest.json` |

## Runtime Flow

1. A production desktop build checks 2.5 seconds after startup and every hour; Settings can trigger a manual check.
2. The updater reads the latest static manifest from GitHub and returns only a newer compatible platform release.
3. Skipped versions remain silent during automatic checks; manual checks always reveal them.
4. “立即更新” downloads in the background, reports progress, and installs only after signature verification.
5. Windows uses NSIS passive mode and exits/restarts through the installer. macOS replaces the app and relaunches it.

## Persistent Data

- Webview local storage: `careeros.updates.skipped-version` and `careeros.updates.last-checked`.
- Local signing-key backup: `~/.tauri/careeros-updater.key`, mode `0600`; never commit it.
- GitHub Actions secret required for releases: `TAURI_SIGNING_PRIVATE_KEY`.
- No SQLite tables or application documents are changed.

## Contracts

- Endpoint: `https://github.com/urbino-M/postdoc-os-desktop/releases/latest/download/latest.json`.
- Manifest platforms: `darwin-aarch64` and `windows-x86_64`.
- macOS artifact: `.app.tar.gz` plus `.sig`.
- Windows artifact: NSIS `-setup.exe` plus `.sig`.
- Public key in `tauri.conf.json` must match the private key used by GitHub Actions.

## Safety Rules

- Tauri signature verification must remain enabled; do not replace this with raw GitHub asset execution.
- Never commit, print, bundle, or return the private signing key.
- A signing-key rotation requires shipping the new public key in an update signed by the old key before switching release signing.
- Automatic checks may be silent; installation requires the user to click “立即更新”.
- Update checks must not read or transmit CVs, databases, OAuth files, or model credentials.

## Debug Checklist

1. Open Settings → 应用更新 and inspect the user-safe status.
2. Confirm the latest GitHub Release contains `latest.json`, both updater bundles, and both `.sig` files.
3. Compare the installed version with `latest.json.version` and confirm the matching platform key.
4. Confirm the public key and CI signing key are a pair without printing the private key.
5. Inspect Tauri updater logs only after the release contract is complete.

## Validation

- Local helpers/UI: `npm test -- src/updates/updateCore.test.ts` and `npm run typecheck`.
- Native registration: `cargo test --manifest-path src-tauri/Cargo.toml --locked`.
- macOS application: `npm run desktop:verify`.
- Release channel: tagged GitHub Actions build must verify the DMG/NSIS install and publish a parseable `latest.json` whose signatures match the uploaded artifacts.

## Common Change Routes

| Change | Start here | Then inspect | Usually avoid |
|---|---|---|---|
| Change prompt, cadence, or skip behavior | `src/updates/UpdateManager.tsx` | `updateCore.test.ts`, Settings card | Rust business modules |
| Change release asset naming | `.github/workflows/release.yml` | `latest.json` generation and Tauri target keys | UI state |
| Add another desktop architecture | release build matrix | manifest platform key and updater artifact | pretending an unbuilt target is supported |

## Known Coupling

- The installed application version, Git tag, package version, and manifest version must match.
- The release workflow owns update artifact creation; `UpdateManager` owns runtime presentation only.
- The updater plugin owns download and cryptographic verification; UI code must not bypass it.

## Out of Scope

- Application database/schema migrations.
- Delta updates, forced updates, private-repository authentication, telemetry, and background service installation.
- Apple notarization or Windows Authenticode; updater minisign verification is separate from operating-system publisher trust.
