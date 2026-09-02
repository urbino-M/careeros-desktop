# Model Provider Routing Handoff

## Scope

This subsystem connects OpenAI Responses-compatible model services to the bundled Codex App Server. It owns URL/key validation, model discovery, provider persistence, generated Codex configuration/catalog data, provider-aware task routing, and disconnect behavior. It does not translate Chat Completions, choose cross-provider fallbacks, or own task business prompts.

## Architecture

Settings calls the typed Tauri boundary, `providers.rs` validates the remote Responses contract, `db.rs` stores non-secret connection/model metadata, and `secrets.rs` stores the key in a current-user CareerOS credentials file. At execution time `codex.rs` generates an app-local configuration snapshot/catalog, passes those values as App Server `-c` overrides, and starts one client per provider/account. Scheduler jobs retain their provider/account/model snapshot. See Source Ownership in `docs/ARCHITECTURE.md`.

## Source Ownership

| Area | Primary files | Responsibility |
|---|---|---|
| Entry point | `src/pages/SettingsPage.tsx`; `src/api.ts`; provider commands in `src-tauri/src/lib.rs` | Collect URL/key, show validation state, cross the Tauri boundary |
| Domain/service | `src-tauri/src/providers.rs`; `src-tauri/src/codex.rs`; provider call sites in `src-tauri/src/scheduler.rs` | Validate Responses compatibility, generate Codex config/catalog and `-c` overrides, select the correct App Server client |
| Persistence/integration | `src-tauri/src/db.rs`; `src-tauri/src/secrets.rs`; `src-tauri/migrations/0010_responses_model_providers.sql` | Store provider/model metadata and opaque credential references separately from secret values |

## Runtime Flow

1. The user enters Base URL and API Key or selects the DeepSeek URL preset.
2. The backend requires HTTPS except for loopback HTTP, calls `GET /models`, and performs a minimal `POST /responses` probe.
3. The key is saved under `model-provider:<provider-id>:api-key`; SQLite stores only its reference, normalized URL, validation result, models, and capabilities.
4. A job snapshots provider/account/model. `CodexManager` loads that snapshot, writes a non-secret config snapshot/catalog below the app-specific Codex home, passes the config as App Server `-c` overrides, injects the key through `env_key`, and starts or reuses the matching client.
5. Disconnect disables the provider, resets affected task defaults to OpenAI, removes the file-backed secret, and invalidates the cached client.

## Persistent Data

- SQLite: `model_providers`, `provider_accounts`, `provider_models`, `task_model_defaults`, and provider/account/model columns on `native_jobs`.
- Generated files: `$POSTDOCOS_DATA_DIR/codex/providers/<id>-config.toml` and `<id>-models.json`; both are non-secret and may be regenerated.
- Credentials file: `<data-root>/credentials/secrets.json`; the directory is `0700` and file is `0600` on Unix, while Windows receives an explicit current-user-only ACL. Writes use same-directory temporary files and atomic replacement. Reference: `model-provider:<id>:api-key`.
- Codex OAuth: `<data-root>/codex/auth.json`, selected through `cli_auth_credentials_store = "file"`.
- Provider IDs are `deepseek` for the official DeepSeek host and deterministic `relay-<url-hash>` IDs for other URLs.

## Contracts

- Tauri: `get_model_providers`, `connect_responses_provider`, `disconnect_responses_provider`, `save_task_model_default`.
- Rust: `ProviderConnectionRequest`, `ProviderDiscovery`, `ProviderInfo`, `ProviderRuntimeConfig`, `CodexTaskRequest`.
- TypeScript: `ProviderConnectionRequest`, `ProviderInfo`, `ProviderModelInfo`.
- External API: Bearer-authenticated `GET <base>/models` and `POST <base>/responses`.
- Codex config: custom `model_providers.<id>` with `base_url`, `env_key`, and `wire_api = "responses"`; `model_catalog_json` supplies discovered model metadata. Bundled Codex 0.144.3 receives these through global `-c` overrides because `--profile` is rejected for `app-server`.

## Safety Rules

- Never write, log, serialize, or return an API key after the connect request.
- Remote HTTP is forbidden; only loopback HTTP is allowed for a local relay.
- A provider is enabled only after both model discovery and a real Responses probe succeed.
- Do not route a job to a provider/account/model other than its persisted snapshot.
- Do not silently fall back to another paid provider.

## Debug Checklist

1. Inspect the provider card's validation message and normalized Base URL.
2. Confirm the provider, enabled account, model, and task default rows in SQLite.
3. Confirm the credentials-file reference exists without printing its value.
4. Inspect the generated config snapshot/catalog and verify `env_key`, provider ID, model slug, and App Server overrides agree.
5. Confirm the job snapshot and cached client key use the same provider/account.
6. Expand to Codex App Server stderr only after the stored contracts agree.

## Validation

- Local UI: `npm run typecheck`.
- Provider/domain: `cd src-tauri && cargo test providers::tests`.
- Persistence: `cd src-tauri && cargo test migration::tests` plus focused DB/scheduler tests when defaults or snapshots change.
- Codex runtime: `cd src-tauri && cargo test codex::tests` and a manual connection with a user-supplied key; automated tests must not use a real external key.
- Release packaging is out of scope unless explicitly requested.

## Common Change Routes

| Change | Start here | Then inspect | Usually avoid |
|---|---|---|---|
| Add provider-specific metadata | `ProviderFlavor` in `src-tauri/src/providers.rs` | catalog generation and provider tests | scheduler business workflow |
| Change connection form/status | `ProviderConnectionSettings` in `src/pages/SettingsPage.tsx` | `src/types.ts`, `src/api.ts`, Tauri command | Codex process internals unless contract changes |
| Change task provider routing | `CodexTaskRequest`/`CodexManager` in `src-tauri/src/codex.rs` | scheduler snapshot and DB runtime config | material renderers, Gmail |
| Add Chat Completions support | new gateway boundary and an explicit architecture decision | provider validation, lifecycle, security, tests | pretending `wire_api` supports a value Codex rejects |

## Known Coupling

- Scheduler owns immutable execution snapshots; Codex owns turning the snapshot into a client/profile.
- DB owns durable provider metadata; the current-user credentials file owns secret bytes.
- Generated model catalogs mirror remote discovery but add conservative Codex metadata; provider-specific capabilities belong in `providers.rs`.

## Out of Scope

- Chat Completions-to-Responses conversion.
- MiniMax/GLM-specific endpoints or authentication headers.
- Automatic retry on a different provider, account pooling, usage billing, or proxy analytics.
- Editing the user's global `~/.codex` configuration.
