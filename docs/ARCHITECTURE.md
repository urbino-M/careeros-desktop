# CareerOS Desktop Architecture

A map of the current implementation. Developer workflow and validation policy live in [AGENTS.md](../AGENTS.md); detailed subsystem contracts live in linked handoffs. Confirm symbols against code when changing behavior.

## System map

Tauri 2 + React/TypeScript + Rust. SQLite holds application state; the resolved application data directory holds profiles, task workspaces, credentials and material versions. Codex, Typst, templates and the runtime application-agent skill are bundled resources.

```text
React page/component
  → src/api.ts (Tauri invoke)
  → src-tauri/src/lib.rs (commands + AppState)
  → owning Rust service
      ├─ db.rs → SQLite
      ├─ scheduler.rs → materials.rs workspace → codex.rs
      │                                      → workflows.rs / materials.rs import
      ├─ typst.rs / cover_letter.rs → bundled Typst → PDF
      └─ gmail.rs + secrets.rs → Gmail draft API
```

`src/types.ts` manually mirrors serialized Rust request/result shapes; there is no generated-binding layer. Command names/registration live in `lib.rs`, wire semantics in Rust types/serde and `src/api.ts`.

Startup resolves `AppPaths`, initializes/migrates data, constructs managers, starts the scheduler and registers commands. Without a native database, migration imports a discovered legacy database or creates an empty foundation before applying native migrations.

## Source Ownership

Rust paths below are relative to `src-tauri/src/`.

| Area | Entry points / owners | Responsibility |
|---|---|---|
| UI and bridge | `src/App.tsx`, `src/pages/`, `src/components/`, `src/styles.css`, `src/themes.css`; `src/uiPreferences.ts`, `src/i18n/`; `src/api.ts`, `src/types.ts`; `lib.rs` | Routing, presentation, local interface preferences and explicit Tauri contracts; commands delegate domain work |
| Onboarding | `src/pages/OnboardingPage.tsx`, `onboarding.rs` | CV upload, local extraction/Mac OCR, optional preferences and source-profile versions |
| Opportunities | `workflows.rs`, `opportunity_identity.rs`, `models.rs` | Structured result validation/import, Postdoc identity evidence shared by search and continuation, domain/result shapes |
| Contacts and outreach | `src/pages/ApplicationDetailPage.tsx`, `db.rs`, `workflows.rs`, `materials.rs` | Manual status, reply persistence/decisions, outreach edits and generated reply artifacts; not Gmail authentication |
| Materials | `materials.rs`; artifact queries in `db.rs` | Workspace/input preparation, target-owned copies, revisions, backups, base-hash conflict checks and publication |
| CV | `cv_schema.rs`, `typst.rs`, `src-tauri/resources/templates/cv.typ` | Content/schema policy, user-directed structure, configurable references, page/layout preflight and rendering |
| Cover Letter | `cover_letter.rs`, `src-tauri/resources/templates/cover-letter.typ` | Editable text, Typst data and PDF rendering; versioning remains in materials |
| Scheduler | `scheduler.rs`, `src/pages/AutomationPage.tsx` | Enqueue/retry, active-job deduplication, concurrency, leases/timeouts, progress/checkpoints and dispatch; not business-result schemas |
| Codex/providers | `codex.rs`, `providers.rs`, `paths.rs` | Bundled App Server lifecycle, login/model discovery, Responses provider configuration and execution |
| SQLite/startup | `db.rs`, `migration.rs`, `paths.rs`, `src-tauri/migrations/` | Queries/transactions, legacy compatibility, backed-up migrations and path resolution; inspect registered migrations for current version |
| Gmail/secrets | `gmail.rs`, `secrets.rs`; settings/draft panels | Desktop OAuth, private credentials, approval-bound MIME attachments and remote draft creation |
| Packaging/updates | `script/build_and_run.sh`, `src-tauri/tauri.conf.json`, `src/updates/`, `.github/workflows/release.yml` | Local build/run/install modes, signed update flow, platform packaging and release metadata |

Bundled files are under `src-tauri/resources/`: `runtime/` for executables/licenses, `templates/` for documents, and `skills/` for runtime agent behavior.

## Change Routing

Use the ownership table to choose the first implementation, then follow actual calls:

- **Local UI:** start in the relevant page/panel. An appearance-only change does not require backend changes.
- **Status/replies:** manual updates go through `db.rs`; Agent decisions/imports through `workflows.rs`; material changes through `materials.rs`.
- **CV failure:** content policy → `cv_schema.rs`; layout → `typst.rs`/template; revision/base conflict → `materials.rs`; missing retry/repair turn → `scheduler.rs`.
- **Duplicate/incomplete opportunity:** identity → `opportunity_identity.rs`; import/publication → `workflows.rs`; displayed records → `db.rs` and ApplicationsPage.
- **Model/auth failure:** connection/configuration → `providers.rs`/`secrets.rs`; child-process request/session → `codex.rs`; job dispatch → `scheduler.rs`. Current custom-provider transport is Responses; other protocol support must be verified or implemented, not assumed.
- **Persistent/API field:** start with its owning Rust type or storage layer, then follow the exposed Tauri path. A UI need alone is not a reason for a schema migration.
- **Build/update issue:** inspect the selected script mode, bundle/runtime resources and release workflow. See [desktop updates](features/desktop-updates.md).

These are starting points, not prohibitions on a necessary cross-module fix.

## Runtime and data flows

### Tasks and opportunities

1. Enqueue snapshots profile/preferences and model settings into an isolated workspace and `native_jobs`. `CAREEROS_TASK.json` is the runtime input contract.
2. Scheduler dispatches Codex and records real events, leases and checkpoints. The frontend refreshes on `careeros://jobs-changed`.
3. Search saves source-verified discoveries before completing materials. `workflows.rs` imports results; missing materials remain pending and zero matches is a reviewable outcome.
4. New searches and opportunity continuations share Same/Different/Review identity decisions. Uncertain matches do not silently merge or create another application card; contact state remains independently owned.
5. Retry retains source evidence and saved outputs. Same-provider retries can resume the original thread; a provider change uses a compatible new thread with workspace evidence. Scoped continuation is a separate task, not automatically a resumed historical search.

See [Postdoc pipeline](features/postdoc-pipeline.md) for freshness, contact-status precedence, scoped continuation, identity compatibility and publication contracts.

### CV and material revisions

Uploaded CV claims are user-provided evidence, not independently verified facts. New uploads affect future tasks; historical task snapshots and materials remain intact. Mac supports local OCR; Windows currently needs text-layer PDFs.

The Agent selects source-backed content for the target, discipline and language. References follow available source information and user customization; no fixed referee count is required. Page count and content/layout quality are validated locally; section and entry counts are not locked by current or legacy templates. See [CV generation policy](features/cv-generation-policy.md) for exact defaults and compatibility.

Manual and Agent revisions go through `materials.rs`; revisions check the trusted base SHA-256 before replacing current material. Direct email editing shares the same draft and base-hash flow as the revision editor. Render into a fresh version directory and publish artifact pointers transactionally only after preflight. Searches preserve existing packages; explicit revisions own replacement. Invalid/missing revision outputs and CV layout failures share a bounded original-thread repair path. Changing retry settings updates the instruction, never silently rebases the source or discards conflict checks.

### Interface language and theme

`uiPreferences.ts` owns device-local Chinese/English and light/dark preferences (`careeros-locale`, `careeros-theme`). `InterfacePreferences` exposes the same controls in the shell, settings and onboarding. Switching updates React subscribers without remounting forms; it does not change material language, source content or Agent requests. Explicit UI text uses `src/i18n/` catalogues; research facts, mail/CV bodies and unknown diagnostic text retain their original language. `themes.css` layers the light palette and adaptive icon surfaces over the existing dark styles. `public/theme-init.js` applies the stored palette before rendering without relaxing CSP.

### Gmail drafts

Settings imports the OAuth client and connects Gmail. Draft creation requires the current CV's approval hash, builds the attachment and records the draft. This path **does not send mail or automatically change contact status**. Secrets and remote transport belong to `gmail.rs`/`secrets.rs`, not CV rendering.

## Persistence and compatibility

`AppPaths` uses `CAREEROS_DATA_DIR` when set, otherwise the platform application-data directory under CareerOS. Moving the source checkout does not itself move user data.

| Data-root location | Contents |
|---|---|
| `database/careeros.sqlite3` | Native state plus retained legacy tables; current task views use `native_jobs` |
| `profile/`, `generated/` | Source profiles/preferences and target-owned material versions |
| `workspaces/` | Task snapshots, output and resumable evidence |
| `codex/` | App-specific Codex home/runtime skill; `auth.json` may hold OAuth credentials |
| `codex/providers/` | Generated non-secret provider configuration/model catalogs; `codex.rs` applies configuration to App Server |
| `credentials/secrets.json` | Provider/Gmail secrets; atomic writes, Unix directory/file modes 0700/0600 or Windows current-user-only ACL |
| `backups/` | Migration/artifact backups; inspect the owning operation for its precise backup path |

Provider API keys stay outside SQLite/config snapshots and are passed only to the relevant provider child through its configured environment variable. Cache/log paths are separately resolved in `paths.rs`.

Migrations are additive/versioned with compatibility defaults and pre-migration backups, including WAL-aware SQLite backup. Preserve legacy readability, artifact ownership, snapshot/base-hash checks and atomic publication. Implemented safeguards and their tests are authoritative; developer safety/validation rules are centralized in [AGENTS.md](../AGENTS.md).
