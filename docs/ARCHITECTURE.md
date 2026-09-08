# CareerOS Desktop Architecture

## Purpose

This document is the repository map for planning changes. Use Source Ownership to identify the owning module, then Change Routing to choose the first files and smallest validation. It is not a requirement to inspect every listed file for every task.

## System at a Glance

CareerOS Desktop is a Tauri 2 application with a React/TypeScript frontend and a Rust backend. It stores native application state in SQLite and task/material files under the resolved application data directory. Codex and Typst executables, document templates, and the runtime application-agent skill are bundled as resources.

```text
React page/component
    ↓ typed call
src/api.ts
    ↓ Tauri invoke
src-tauri/src/lib.rs command + AppState
    ↓
owning Rust subsystem
    ├─ db.rs / SQLite
    ├─ materials.rs / generated files and revisions
    ├─ scheduler.rs → materials.rs/workspace → codex.rs → materials.rs/workflows.rs
    ├─ typst.rs or cover_letter.rs → bundled Typst/templates → PDF
    └─ gmail.rs / secrets.rs → Gmail draft API and private credentials file
```

Application startup in `src-tauri/src/lib.rs` resolves `AppPaths`, initializes/migrates data, constructs Codex, scheduler, and Gmail managers, starts the scheduler, stores them in `AppState`, and registers the Tauri commands. When no native database exists, startup imports a discovered legacy database or creates an empty database from the versioned legacy-foundation schema before applying native migrations.

## Runtime and Data Flows

### Read and Update a Contact Target

1. A page calls a wrapper in `src/api.ts`.
2. The matching command in `src-tauri/src/lib.rs` reads `AppState` and delegates to `db.rs` or the owning service.
3. `db.rs` reads or updates SQLite and resolves target-owned artifact paths below the application data root.
4. Rust serializes camelCase data matching interfaces in `src/types.ts`.

### Background Task

1. `AutomationPage.tsx` or an application-detail panel creates an `EnqueueRequest` through `src/api.ts`.
2. `scheduler.rs` snapshots provider/model settings into `native_jobs`, rejects duplicate active keys, dispatches with bounded concurrency, and records lifecycle events, execution deadlines, lease heartbeats, and checkpoints.
3. `materials.rs` prepares an isolated workspace and runtime input contract (`CAREEROS_TASK.json`); material-revision tasks snapshot the source SHA-256 in both the contract and the trusted job payload. This is separate from the developer Task Contract in `AGENTS.md`.
4. For `internship_search`, the task contract selects the independent Internship profile and runtime skill. Codex searches public recruitment information, including accessible LinkedIn / X posts, then verifies against official Web / ATS sources; there is no native channel setup, login, or pre-search step.
5. `codex.rs` runs or resumes the bundled Codex App Server task.
6. `materials.rs` rejects an Agent revision if the live material no longer matches its trusted base SHA-256, then applies the revision; other jobs use `workflows.rs` to validate and import structured business results.
7. CV/Cover Letter revisions may invoke `typst.rs` or `cover_letter.rs` to regenerate PDFs.
8. The job moves to `needs_review`; the frontend refreshes on `careeros://jobs-changed`.

### Manual Material Revision

1. `ApplicationDetailPage.tsx` submits a `ManualRevisionRequest` through `src/api.ts`.
2. `save_manual_material` in `lib.rs` delegates versioning and target-owned copies to `materials.rs`.
3. CV data invokes `typst.rs`; Cover Letter text invokes `cover_letter.rs`.
4. Revision metadata and artifact ownership are persisted in SQLite.

### Gmail Draft

1. Settings connect an OAuth desktop client through `SettingsPage.tsx` and `gmail.rs`; secrets are stored through `secrets.rs` in the current-user CareerOS credentials file.
2. `ApplicationDetailPage.tsx` parses the reviewed outreach email and requests a draft.
3. `gmail.rs` requires an approved current CV hash, builds the MIME attachment, creates a Gmail draft, and records it locally.
4. This path does not send mail and does not automatically change contact status.

## Repository Layout

- `src/App.tsx`: hash routing and top-level page selection.
- `src/pages/`: feature-facing React pages.
- `src/components/`: shared presentation and controls.
- `src/api.ts`: the only frontend Tauri invoke wrapper.
- `src/types.ts`: shared frontend route and Tauri payload/result shapes.
- `src-tauri/src/`: Rust application state, commands, domain services, integrations, persistence, and rendering.
- `src-tauri/migrations/`: versioned native SQLite schema.
- `src-tauri/resources/templates/`: bundled Typst templates.
- `src-tauri/resources/runtime/`: bundled Codex and Typst executables and licenses.
- `src-tauri/resources/skills/`: runtime agent skill copied into the app-specific Codex home.
- `src/pages/ApplicationDetailPage.test.ts` and colocated Rust `#[cfg(test)]` modules: current regression tests.
- `script/build_and_run.sh`: development, verification, and release orchestration.
- `.github/workflows/release.yml`: tag/version gate, platform runtime acquisition, macOS/Windows packaging, and atomic GitHub Release publication.

# Source Ownership

| Area | Primary files | Responsibility | Boundary |
|---|---|---|---|
| Application UI | `src/App.tsx`; `src/components/Shell.tsx`; `src/components/Ui.tsx`; `src/pages/*.tsx`; `src/styles.css` | Routing, navigation, presentation, page-local state, user interactions | UI-only changes should not enter Rust, SQLite, or scheduler code unless behavior/data contracts actually change. |
| Frontend API bridge | `src/api.ts` | Central typed wrappers around Tauri `invoke` calls | Does not own business rules, persistence, or view state. Do not bypass it with page-local `invoke` calls. |
| Shared TS contracts | `src/types.ts` | Frontend representations of routes, requests, and Rust command results | Does not own rendering or backend behavior; change it only when a real caller or cross-boundary shape changes. |
| Tauri command boundary | `src-tauri/src/lib.rs` | `AppState`, startup wiring, command arguments/results, delegation, and command registration | Keep domain logic in owning modules; ordinary page layout changes do not belong here. |
| First-run onboarding | `src/pages/OnboardingPage.tsx`; gate in `src/App.tsx`; `src-tauri/src/onboarding.rs` | Career-stage and discipline context, target preferences, resumable onboarding state, and local CV source import | Stores profile files only; it does not own model transport, Gmail OAuth, opportunity persistence, or automatic verification of uploaded CV claims. |
| Application domain | `src-tauri/src/models.rs`; `src-tauri/src/workflows.rs` | Core target/job shapes, structured agent-result contracts, validation, deduplication, and result import | Does not own generic scheduling, UI presentation, or external transport authentication. |
| Search evidence | `src-tauri/src/models.rs`; `src-tauri/src/workflows.rs` | Source provenance, deduplication, and official-primary-evidence verification for Codex web-search results | Source channel labels remain readable for old records; they do not imply installed adapters or authenticated social accounts. |
| Internship profile | `src-tauri/src/internship.rs`; Internship strategy UI | Independent `profile/internship.json`, optional profile-local CV, and isolated workspace copy; legacy RSS metadata is preserved but not fetched | Must not reuse Postdoc `master_profile.json` or own Postdoc onboarding. |
| CV | `src-tauri/src/cv_schema.rs`; `src-tauri/src/typst.rs`; `src-tauri/resources/templates/cv.typ` | Target-isolated CV normalization and deduplication, exact-two-page preflight/rendering, PDF generation, and CV revision persistence | A target CV must come from that target's Agent selection. It must never be completed from another contact's CV. Visibility, icons, and preview toggles belong to the UI. |
| Cover Letter | `src-tauri/src/cover_letter.rs`; `src-tauri/resources/templates/cover-letter.typ`; relevant panels in `src/pages/ApplicationDetailPage.tsx` | Cover Letter content assembly, text/source persistence, Typst layout, PDF regeneration, and preview controls | Does not own scheduler lifecycle, Gmail OAuth, or unrelated CV rendering. |
| Email / Outreach | email/reply/artifact panels in `src/pages/ApplicationDetailPage.tsx`; `src-tauri/src/materials.rs`; `src-tauri/src/workflows.rs`; reply persistence in `src-tauri/src/db.rs` | Display/edit/revision of outreach materials, inbound reply persistence/follow-up, and agent-produced email artifacts | Gmail authentication and remote draft transport belong to Gmail; ordinary outreach editing must not change scheduler infrastructure. |
| Materials | `src-tauri/src/materials.rs`; artifact queries in `src-tauri/src/db.rs`; material/revision panels in `src/pages/ApplicationDetailPage.tsx` | Target-owned material copies, manual/agent revisions, backups, diffs, task workspaces, and artifact path safety | Does not own general job dispatch or document layout beyond triggering the owning renderer. |
| Scheduler | `src-tauri/src/scheduler.rs`; `src/pages/AutomationPage.tsx`; scheduler commands in `src-tauri/src/lib.rs` | Background job queue, concurrency, lifecycle, cancellation/retry/review, progress, checkpoints, and dispatch | Ordinary UI/material editing must not touch scheduler unless background behavior changes. Scheduler does not own business-result schemas. |
| SQLite | `src-tauri/src/db.rs`; `src-tauri/src/migration.rs`; `src-tauri/migrations/*.sql` (latest: `0012_search_channels.sql`) | Connections, queries, status/artifact/job persistence, legacy import, native schema, verification/source provenance, backups, and compatibility | Do not introduce schema changes for a UI-only need or rewrite an already-applied migration; schema evolution must be explicit, versioned, and migration-safe. |
| Typst / PDF | `src-tauri/src/typst.rs`; rendering portions of `src-tauri/src/cover_letter.rs`; `src-tauri/resources/templates/*.typ`; bundled Typst under `src-tauri/resources/runtime/` | Locate bundled Typst, render document sources, enforce output/page rules, and persist PDF artifacts | Does not own UI preview visibility, job lifecycle, or email transport. Content semantics remain with CV/Cover Letter owners. |
| Codex integration | `src-tauri/src/codex.rs`; `src-tauri/src/providers.rs`; Codex call sites in `src-tauri/src/scheduler.rs`; `src-tauri/src/paths.rs` | Bundled App Server process, login/account/model calls, task run/resume/interrupt, provider capabilities, and app-specific Codex home | The runtime skill owns application-agent behavior, not developer workflow; presentation and material rendering do not belong here. |
| Gmail | `src-tauri/src/gmail.rs`; `src-tauri/src/secrets.rs`; Gmail settings/draft panels in `src/pages/SettingsPage.tsx` and `src/pages/ApplicationDetailPage.tsx` | OAuth setup, file-backed credentials, CV approval hashes, MIME construction, remote draft creation, and draft records | Draft-only integration: no send interface and no automatic contact-status transition. It does not own outreach content generation. |
| Desktop updates | `src/updates/`; update card in `src/pages/SettingsPage.tsx`; updater/process registration in `src-tauri/src/lib.rs`; `src-tauri/tauri.conf.json`; `.github/workflows/release.yml` | Background release checks, signed update prompts/download/install, skipped-version preference, updater artifacts, and `latest.json` publication | Does not own application data migration, business-task scheduling, or unsigned package execution. |
| Paths and migration startup | `src-tauri/src/paths.rs`; `src-tauri/src/migration.rs`; startup in `src-tauri/src/lib.rs` | Application Support/cache/log/runtime paths, built-in skill installation, legacy discovery/import, and startup migration | Do not touch for ordinary feature work; changes can affect all persisted user data and packaged runtime resources. |

# Change Routing

| User asks to change... | Start here | Then inspect | Usually avoid |
|---|---|---|---|
| Application detail UI | Relevant panel in `src/pages/ApplicationDetailPage.tsx` | `src/styles.css`, nearest test, and `src/types.ts` only if displayed data changes | `src-tauri/src/scheduler.rs`, migrations, unrelated Rust services |
| First-run questions or CV import | `src/pages/OnboardingPage.tsx` | `src-tauri/src/onboarding.rs` → commands in `src-tauri/src/lib.rs` → `src/api.ts`/`src/types.ts`; `src-tauri/src/materials.rs` only for Agent workspace copying | SQLite schema, migration startup, scheduler lifecycle |
| CV visibility or preview toggle | `CvPanel` in `src/pages/ApplicationDetailPage.tsx` | `src/styles.css` and `src/pages/ApplicationDetailPage.test.ts` if behavior is testable | `src-tauri/src/cv_schema.rs`, `src-tauri/src/typst.rs`, scheduler, SQLite |
| CV content/schema | `src-tauri/src/cv_schema.rs` and `src-tauri/src/typst.rs` | `src-tauri/src/materials.rs`, `src-tauri/resources/templates/cv.typ`, command/API/types only if the contract changes | scheduler internals and unrelated UI cleanup |
| Cover Letter editing | Cover Letter/material panels in `src/pages/ApplicationDetailPage.tsx` | `src-tauri/src/materials.rs`, `src-tauri/src/cover_letter.rs`, `src-tauri/src/lib.rs` command wrapper | scheduler redesign, database schema, CV engine |
| PDF regeneration | `src-tauri/src/typst.rs` for CV or `src-tauri/src/cover_letter.rs` for Cover Letter | `src-tauri/src/materials.rs`, corresponding template under `src-tauri/resources/templates/`, calling command | unrelated pages, Gmail, scheduler unless job-based regeneration changes |
| Outreach email content | email/artifact panels in `src/pages/ApplicationDetailPage.tsx` | `src-tauri/src/materials.rs` or `src-tauri/src/workflows.rs` according to manual vs agent output | CV engine, Gmail OAuth, scheduler infrastructure |
| Inbound reply or follow-up | `ReplyPanel` in `src/pages/ApplicationDetailPage.tsx` | `src/api.ts` → `src-tauri/src/lib.rs` → `src-tauri/src/db.rs`; inspect `src-tauri/src/workflows.rs`/scheduler only if agent follow-up behavior changes | Gmail OAuth, Typst/PDF, unrelated contact records |
| Reply decision routing / shelving | `src-tauri/src/workflows.rs` | `src-tauri/src/db.rs`, `src-tauri/src/materials.rs`, runtime agent skill, status UI/types | scheduler lifecycle, Gmail, document rendering |
| Application/contact status | status action in the relevant page | `src/api.ts` → `src-tauri/src/lib.rs` → `src-tauri/src/db.rs`; `src/types.ts` if values change | Typst, Codex, scheduler |
| Internship public-web search | `src-tauri/src/workflows.rs`, `src-tauri/src/materials.rs`, and Internship runtime skill | `internship.rs`, Internship UI, and scheduler only for task lifecycle changes | Native channel adapters, search-tool installation, social-account login, direct submission, schema changes for provenance labels |
| Scheduled task behavior | `src-tauri/src/scheduler.rs` and `src/pages/AutomationPage.tsx` | `src-tauri/src/workflows.rs`, `src-tauri/src/materials.rs`, or `src-tauri/src/codex.rs` only for the affected job type; command/API contracts | unrelated renderers, Gmail, general UI cleanup |
| Tauri API | command in `src-tauri/src/lib.rs` | owning Rust type/service → `src/api.ts` → `src/types.ts` → caller; command registration | unrelated commands and persistence changes not required by the contract |
| Database field/schema | `src-tauri/migrations/` and `src-tauri/src/migration.rs` | `src-tauri/src/db.rs`, `src-tauri/src/models.rs`, then API/types/UI only if exposed | opportunistic UI or scheduler refactors |
| Codex runtime/process behavior | `src-tauri/src/codex.rs` | `src-tauri/src/providers.rs`, `src-tauri/src/scheduler.rs`, `src-tauri/src/paths.rs`; runtime skill only for business-agent behavior | document renderers and unrelated pages |
| Responses model provider connection | provider connection panel in `src/pages/SettingsPage.tsx` | `src/api.ts` → provider commands in `src-tauri/src/lib.rs` → `src-tauri/src/providers.rs`/`secrets.rs`/`db.rs`; `codex.rs` and scheduler only for execution routing | Chat Completions translation, automatic provider fallback, Gmail, document renderers |
| Gmail connection or draft creation | `src-tauri/src/gmail.rs` | `src-tauri/src/secrets.rs`, Gmail commands, `src/api.ts`, `src/types.ts`, Settings or draft panel | scheduler, CV generation internals, automatic status updates |
| Desktop update behavior or release metadata | `src/updates/UpdateManager.tsx` | Tauri updater config/plugin permissions, `.github/workflows/release.yml`, `docs/features/desktop-updates.md` | SQLite, application-data migration, scheduler, provider credentials |
| Startup paths or legacy migration | `src-tauri/src/paths.rs` or `src-tauri/src/migration.rs` | migration SQL, `src-tauri/src/lib.rs` setup, migration tests | ordinary feature/UI modules |

## Contract Ownership

The frontend/backend boundary is deliberately explicit:

1. `src-tauri/src/lib.rs` owns command names, arguments, result delegation, and registration.
2. The subsystem module owns the Rust request/result/domain type and validation.
3. `serde(rename_all = "camelCase")`, command `rename_all`, optional fields, and defaults define wire semantics.
4. `src/api.ts` owns the frontend invocation name and argument object.
5. `src/types.ts` mirrors the serialized shape used by callers.
6. The page/component owns presentation and interaction.

A contract change is incomplete until every applicable layer is checked. Do not add persistence simply because a UI type changed.

Use the shape of the change to keep inspection targeted:

| Contract change | Required inspection |
|---|---|
| Existing request field | Owning Rust request type and validation, command argument mapping if separate, `src/api.ts`, affected caller, and `src/types.ts` when the request type is shared there |
| Existing response field | Owning Rust result/model serialization, `src/types.ts`, and affected caller; inspect `src/api.ts` only for wrapper/generic changes |
| New or renamed command | Owning Rust service, command in `lib.rs`, `tauri::generate_handler!`, `src/api.ts`, TypeScript request/result types, and direct callers |
| Persisted field | Persistence and migration layers first, then the command/API/UI layers only when the field is exposed |

## Persistence and Compatibility

`AppPaths` resolves the data root to `CAREEROS_DATA_DIR` when set, otherwise the platform application-data directory. Important persisted locations include:

- `database/careeros.sqlite3`: SQLite application state;
- `generated/`: target material and document outputs;
- `profile/`: candidate source-of-truth files used in agent workspaces;
- `profile/internship.json`: independent Internship search preferences, optional
  profile-local CV path, and legacy RSS feed list retained without fetching; it is not derived from Postdoc
  `master_profile.json`;
- `workspaces/`: isolated Codex task inputs/outputs and resumable results;
- `codex/`: app-specific Codex home and installed runtime skill;
- `codex/providers/<provider-id>-config.toml` and `codex/providers/<provider-id>-models.json`: generated non-secret Codex configuration snapshots/catalogs; App Server receives the same values through `-c` overrides because version 0.144.3 does not accept `--profile` for `app-server`; API keys stay in the private credentials file and are exposed to only that provider's child process through its configured environment variable;
- `backups/`: migration and artifact backups;
- `credentials/secrets.json`: provider and Gmail secrets, stored outside SQLite using atomic replacement; the parent is `0700` and file is `0600` on Unix, while Windows receives an explicit current-user-only ACL. `secrets.rs` owns access.
- `codex/auth.json`: Codex OAuth credentials when file storage is enabled; it remains under the app-specific Codex home.

Compatibility rules:

- Existing persisted user data must remain readable unless an explicit migration is implemented.
- Schema changes require a new idempotent/versioned migration path, wiring in `migration.rs`, and targeted migration tests. Treat `0008_native_desktop.sql` and `0009_reply_routing_and_submission_status.sql` as applied history rather than files to rewrite for later schema evolution.
- New serialized fields must define intentional missing/null/default behavior; use `#[serde(default)]` only when it matches the compatibility contract.
- Artifact paths must remain constrained to the application data root and target ownership rules.
- Legacy import and native migration create backups; do not bypass those safeguards in feature code.

CV artifact invariants:

- `profile/master_profile.json` is the candidate-fact source; a target's Agent output is the selection source for that target.
- Formal open opportunities select evidence against the verified recruitment duties and requirements. Prospective-PI records select against the PI's verified research direction without implying an advertised opening.
- `cv_schema.rs` may normalize and deduplicate only within one target. Cross-target CV merging is prohibited.
- Normalized CV sections place selected research outputs/publications first, selected patents second, and selected research projects third. Articles and patents must never render after projects.
- The bundled CV template owns the approved compact spacing: a tight name-to-headline transition, a tight section-title-to-rule transition, and compact bullet-to-entry indentation. It starts the research-project section on page two to balance the ordered two-page layout. It bolds the normalized `cvData.authorName` in publication author lists, while career-stage wording comes only from the verified candidate profile.
- `typst.rs` preflights the normalized selection before import and accepts only exactly two well-filled A4 pages with at least 36 distinct content entries. One page, a sparse second page, and more than two pages are invalid; space must be filled with target-relevant verified evidence rather than repetition, padding, invention, oversized spacing, or unreadably compressed text. A failing CV must not become a newly imported ready-to-contact package.

## Targeted Validation Routes

Choose the narrowest applicable route; commands are run from the repository root unless noted.

| Changed area | Minimal validation starting point |
|---|---|
| Local TypeScript/TSX | `npm run typecheck` |
| Application detail parsing/interaction | `npm test -- src/pages/ApplicationDetailPage.test.ts` |
| Materials | `cd src-tauri && cargo test materials::tests` |
| CV schema / Typst | `cd src-tauri && cargo test cv_schema::tests`; `cd src-tauri && cargo test typst::tests` when rendering behavior changes |
| Cover Letter | `cd src-tauri && cargo test cover_letter::tests` |
| Workflows / deduplication / result import | `cd src-tauri && cargo test workflows::tests` |
| Scheduler | `cd src-tauri && cargo test scheduler::tests` |
| Migration / paths | `cd src-tauri && cargo test migration::tests`; add `cd src-tauri && cargo test paths::tests` when path resolution changes |
| Codex / providers | `cd src-tauri && cargo test codex::tests`; add `cd src-tauri && cargo test providers::tests` for provider behavior |
| Gmail | `cd src-tauri && cargo test gmail::tests` |
| Cross-module Rust behavior | `cd src-tauri && cargo test --lib` only when behavior truly spans several Rust subsystems and filtered tests cannot prove it; an ordinary `lib.rs` command wrapper alone does not require this |
| Release/package | `npm run desktop:verify` or `npm run desktop:release`, only for explicit release work |

Tests are evidence, not ceremony. A filtered test that does not exercise the changed behavior is insufficient even if it passes.

## High-Coupling Review Points

- `ApplicationDetailPage.tsx` contains several independent panels. Keep local UI changes inside the owning panel; its file size alone is not permission to refactor it.
- `lib.rs` is a command registry/orchestration boundary. Do not move domain logic there for convenience.
- `scheduler.rs` coordinates Codex, materials, workflows, and renderers. Touching it is justified only by lifecycle/dispatch/job-flow changes.
- Internship search uses the existing Codex web-search path. The removed native channel extension must not be reintroduced through setup/login commands or pre-search subprocesses. Keep historical source/verification fields and migration 0012 intact.
- `workflows.rs` owns structured result validation and import, while `scheduler.rs` owns execution lifecycle. Keep that split.
- `materials.rs` owns workspace/input preparation plus manual and revision artifact application; `workflows.rs` owns validated import of non-revision structured business outputs. `models.rs` owns serialized `JobSummary` data, while `scheduler.rs` owns `EnqueueRequest` and lifecycle transitions.
- `db.rs` preserves imported legacy tables for compatibility and audit, while current task views query `native_jobs` only. A new view need does not automatically justify a schema change or deletion of imported data.
- The runtime `SKILL.md` constrains generated application materials. Changes to developer workflow belong in `AGENTS.md`, never in the runtime skill.
