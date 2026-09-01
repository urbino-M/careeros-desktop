# Project Instructions

## Operating Model

For every non-trivial change, work in this order:

> Goal → Root Cause → Scope → Source Ownership → Expected Files → Minimal Patch → Minimal Validation → git diff audit

Prefer the smallest correct change that satisfies the request. Do not turn a small fix into a refactor. Architectural improvement opportunities discovered during a Small Change should be reported, not implemented.

## Documentation Hierarchy

- `AGENTS.md`: how Codex should work in this repository.
- `docs/ARCHITECTURE.md`: where software responsibilities live, how data flows, and where a change should start.
- `docs/FEATURE_HANDOFF_TEMPLATE.md`: template for complex, long-lived subsystem handoffs.
- `docs/adr/`: durable architecture decisions only.
- `src-tauri/resources/skills/postdoc-application-agent/SKILL.md`: runtime business behavior for the Postdoc Application Agent. It is not a developer instruction file and must not be treated as one.

Read only the relevant architecture section and directly related sources before expanding the search.

## Before Work

1. Run `git status --short`.
2. Record which dirty files already exist. They are user-owned state.
3. Read the relevant section of `docs/ARCHITECTURE.md`, especially Source Ownership and Change Routing.
4. Inspect the directly related symbols and callers. Do not begin with an undirected repository-wide search.

Never restore, reset, checkout, format, stage, or overwrite user-owned dirty changes. If the requested task must modify an already-dirty file, preserve the pre-existing hunks and add only the smallest task-specific edit; audit the combined diff carefully.

## Task Contract

Before a non-trivial implementation, establish this contract internally:

### Goal

State the exact user-visible or system behavior requested.

### Root Cause

- Bug: identify the real failing condition before editing broadly.
- Feature: write `Not applicable — feature request.`
- If the cause is still unknown, continue targeted diagnosis; do not compensate with speculative cross-module changes.

### Scope

List the behavior that may change.

### Out of Scope

Name adjacent behavior that must remain unchanged, such as scheduler design, database schema, unrelated UI cleanup, generation-pipeline refactors, or dependencies.

### Expected Files

Use `docs/ARCHITECTURE.md` to classify likely files:

- `Expected`: files that should change.
- `Possible`: files needed only if the directly inspected contract requires them.
- `Unexpected`: modules that would indicate scope expansion.

If an Unexpected file becomes necessary, stop and reassess the contract before editing it.

### Minimal Validation

Choose the lowest validation level that can prove the requested behavior without hiding risk.

## Scope Expansion Gate

A Small Change has a default budget of 1–3 implementation source files. This is a diagnostic budget, not a hard limit; tests and documentation still belong in the scope audit. A known cross-boundary contract slice may legitimately need more files, but it must be planned as such before editing rather than discovered through drift.

Reassess scope before any of the following:

- modifying a fourth source file;
- touching the scheduler, database schema, migrations, global state, or shared infrastructure;
- adding a dependency;
- changing a frontend/backend contract;
- extracting or redesigning a shared helper;
- producing a diff materially larger than the Expected Files plan.

Ask:

> Is this technically required to satisfy the user's request, or am I improving adjacent architecture?

If it is adjacent improvement, do not implement it. If it is required, update Expected/Possible/Unexpected files and the validation plan. Ask the user before a meaningful expansion beyond the requested behavior.

## Small Change Workflow

### Step 1 — Read

Read `AGENTS.md`, the relevant `docs/ARCHITECTURE.md` section, and directly related source files. Do not start with a full-repository search.

### Step 2 — Locate

Use symbol search to find the real entry point, caller, contract, and nearest existing test. Check colocated tests in the owning module first; widen the search only if they do not cover the behavior.

### Step 3 — Establish Scope

Write the Goal, Root Cause, Scope, Out of Scope, Expected Files, and Minimal Validation.

### Step 4 — Implement

Use this preference order:

> existing implementation → existing helper → local modification

Avoid introducing a new abstraction, generalized framework, or broad refactor unless the task explicitly requires it.

### Step 5 — Validate

Run only the smallest checks directly related to the changed behavior. Escalate validation when the contract or infrastructure changes.

### Step 6 — Audit Diff

Run `git status --short`, `git diff --stat`, and inspect the relevant diff. For every changed file, answer:

- Why was this file changed?
- Which user requirement requires it?
- Is any hunk unrelated?
- Did actual scope exceed planned scope?

Remove unnecessary changes created by the current task before finishing. Never remove pre-existing user changes.

## Validation Levels

### Level 1 — Local / Small Change

For copy, icons, visibility, local interaction, or a small form/UI change:

- run `npm run typecheck` when TypeScript/TSX changes;
- run the nearest targeted Vitest file when behavior is covered, for example `npm test -- src/pages/ApplicationDetailPage.test.ts`.

Do not run packaging or release checks for a local UI change.

### Level 2 — Contract / Domain Change

For a TypeScript ↔ Rust contract, Tauri command, Rust business rule, or material generation change:

- run the relevant frontend check when the frontend contract or caller changes;
- run a filtered Rust test from `src-tauri`, for example `cargo test materials::tests` or the nearest test-name filter;
- widen to `cargo test --lib` only when the change crosses several Rust modules or no narrower check proves it.

### Level 3 — Infrastructure Change

For scheduler, SQLite, migrations, resource packaging, Codex, Gmail, or secrets, use the subsystem routes and tests in `docs/ARCHITECTURE.md`. Add integration/build validation only when the changed boundary requires it.

### Level 4 — Release

Only for explicit release preparation, use broader checks such as `npm run desktop:verify` or `npm run desktop:release`.

> Release validation must not be used as the default validation for a Small Change.

## Contract Change Checklist

When changing a Tauri boundary, check all applicable points:

- Rust command arguments and return type in `src-tauri/src/lib.rs`;
- the owning Rust request/result/model type, commonly in `src-tauri/src/models.rs`, `scheduler.rs`, `materials.rs`, `typst.rs`, `cover_letter.rs`, or `gmail.rs`;
- `serde` rename/default behavior, especially camelCase conversion and optional values;
- command registration in `tauri::generate_handler!`;
- the wrapper in `src/api.ts`;
- the mirrored TypeScript type in `src/types.ts`;
- every affected page/component caller;
- null, missing, and optional semantics on both sides.

Do not update only one side of a frontend/backend contract.

## Persistence Safety

For persisted models or fields, inspect the supported persisted shapes, current schema/migrations, fixtures or representative old-version samples, defaults, nullable/optional behavior, and backward compatibility. Do not inspect a user's live Application Support data unless the task requires it and the user has authorized that scope. `#[serde(default)]` is one possible compatibility tool, not a mechanical requirement.

> Existing persisted user data must remain readable unless an explicit migration is implemented.

Database schema work belongs in the migration and persistence ownership boundaries documented in `docs/ARCHITECTURE.md`; do not hide schema changes inside an unrelated feature.

## Feature Handoffs and ADRs

Create a feature handoff under `docs/features/` from `docs/FEATURE_HANDOFF_TEMPLATE.md` only for a complex, long-lived subsystem and when at least one is true:

- the subsystem spans frontend, Rust, and storage;
- it has at least five long-lived maintenance files;
- it has a complex runtime lifecycle;
- it owns an external integration;
- agents repeatedly need to rediscover it.

Name an instantiated handoff `docs/features/<kebab-case-feature>.md`; do not create the directory or a handoff until the criteria are met. Do not create handoffs for simple components. Use `docs/adr/` only for decisions that materially constrain future architecture, not ordinary bugs, UI changes, or small implementation choices. A subsystem may have both: its handoff describes current operation, while ADRs preserve why durable constraints were chosen.

## Git Workflow

- Before modification, run `git status --short` and preserve the baseline.
- Do not overwrite or delete existing uncommitted changes.
- After a Small Change, inspect `git diff`.
- Before any requested commit, inspect `git diff --stat` and the relevant diff for scope expansion.
- Do not use `git add .` unless every path has been explicitly reviewed.
- Do not run destructive commands such as `git reset --hard` or `git clean -fd` without explicit authorization.
- Do not rewrite history, force push, or push automatically.
- Do not create a commit unless the user explicitly asks.

## Completion Report

For medium or larger work, report:

- `Summary`: what changed.
- `Root Cause`: the diagnosed cause, or feature-request notation.
- `Scope`: behavior actually changed.
- `Files Changed`: one reason per file.
- `Out of Scope`: important adjacent work not done.
- `Validation`: exact checks run and their results.
- `Scope Audit`: planned file count, actual file count, and any justified expansion.
