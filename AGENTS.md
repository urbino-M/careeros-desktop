# Project Instructions

## Working approach

Use judgment to deliver the requested outcome, not to satisfy a prescribed number of steps or changed files. These instructions are model-agnostic.

- Start with `git status --short`; preserve existing changes. Read the relevant part of [ARCHITECTURE.md](docs/ARCHITECTURE.md) and trace the actual callers, state and nearest tests. Code is evidence; documentation is a map.
- For a bug, establish the failing condition before editing. For a feature, identify the intended behavior and affected contracts. Keep a short internal plan when useful; routine work does not need a formal task contract.
- Make the smallest **complete** correction. Fix the owning mechanism and relevant entry points when a local workaround would leave the same failure elsewhere. Necessary cross-module changes, shared helpers and focused refactors are allowed within the request; explain material tradeoffs.
- Choose inspection depth, implementation and validation by risk. No file-count budget or mandatory one-size-fits-all sequence. Expand investigation when evidence points outside the initial files.
- Ask before materially expanding the user's goal, making a consequential product choice, or taking an action needing new authority. Report unrelated improvements instead of implementing them.
- Audit your diff against the starting state. Keep only task-related edits; do not remove somebody else's work.

## Safety boundaries

- Preserve user-owned dirty changes, imported data, material versions and contact history. Do not reset, delete, bulk-format or overwrite them. If edits overlap, retain their behavior; ask when genuinely incompatible.
- Persisted data must remain readable. Inspect old shapes and missing/null/default semantics; schema changes need a new versioned migration, migration tests and the existing backup safeguards. Do not rewrite applied migrations.
- Inspect live Application Support data only when required and authorized. Prefer fixtures/copies for diagnosis; never expose credentials in logs, prompts or reports.
- Keep artifact paths within their data-root and target-ownership boundaries. Preserve source snapshots, revision base-hash checks and atomic publication.
- No automatic email sending, application submission, commit, push, release or destructive Git operation. Each requires the user's authorization; stage only reviewed paths.

## Contracts and ownership

Keep business behavior in its owning subsystem, not duplicated in UI, command wrappers or scheduler glue.

For a changed Tauri contract, follow the affected path: owning Rust type/validation → `lib.rs` command/registration → `src/api.ts` → `src/types.ts` → callers. Check serialization names, optional fields and defaults on both sides. These bindings are manually maintained; do not assume generated bindings exist.

The bundled `src-tauri/resources/skills/postdoc-application-agent/SKILL.md` controls the **runtime application agent**, not repository development. Editing that runtime skill or business rules is a separate scope from changing developer guidance.

## Validation and delivery

Run checks that exercise the changed behavior, widening coverage when risk warrants it. A passing unrelated test is not validation.

| Change | Starting point |
|---|---|
| Documentation only | Check factual references, links, contradictions and `git diff --check`; no App build needed |
| TypeScript/UI | `npm run typecheck`, nearest `npm test -- <test-file>`; inspect affected UI when appearance/interaction changes |
| Rust/domain | `cargo test --manifest-path src-tauri/Cargo.toml <module>:: --lib`; use `workflows::` to include identity tests |
| Persistence, scheduler, integrations or rendering | Relevant failure/compatibility tests; expand to `cargo test --manifest-path src-tauri/Cargo.toml --lib` or integration tests as needed; inspect representative rendered output for layout changes |
| Requested App build/release | Build on the local Mac first with `npm run desktop:build -- --bundles app`; validate the artifact and relevant behavior before authorized publication |

Respect any requested external `CARGO_TARGET_DIR`. Use local mocks for integrations where possible. Inspect `package.json` and `script/build_and_run.sh` before using broader run/release commands; they may launch, stop or install an App.

Distinguish source changes, tests, packaged artifacts, installation and the running process. Rebuilding a file does not update an already-running App. When diagnosing a reported App failure, verify which build is running; do not claim end-to-end success from compilation alone.

Finish with the outcome, relevant cause, checks/results and remaining limits. Mention material scope deviations, but do not produce a fixed multi-section report for routine work. For App delivery, provide the artifact path and state whether it was installed/launched.

## Documentation

- This file owns developer guidance; [ARCHITECTURE.md](docs/ARCHITECTURE.md) owns the current system map. Avoid repeating either in feature notes.
- Update an existing `docs/features/` handoff when a complex subsystem changes; use [FEATURE_HANDOFF_TEMPLATE.md](docs/FEATURE_HANDOFF_TEMPLATE.md) only when a new handoff is useful for long-term maintenance.
- Reserve `docs/adr/` for durable architecture decisions. Do not turn every fix into a new document or append test-run history to the system map.
