# First-run Onboarding Handoff

## Scope

The first-run flow collects user-authored career context, discipline, goals, constraints, language preference, and an optional CV source. It also routes users to existing model and Gmail authentication. It does not verify CV claims, create opportunities, send email, or submit applications.

## Architecture

`App.tsx` gates the ordinary shell on the persisted onboarding profile. `OnboardingPage.tsx` saves each completed round through typed Tauri wrappers. `onboarding.rs` validates and stores the profile and copies an uploaded CV into the application profile directory. See Source Ownership in `docs/ARCHITECTURE.md`.

## Source Ownership

| Area | Primary files | Responsibility |
|---|---|---|
| Entry point | `src/App.tsx`; `src/pages/OnboardingPage.tsx` | First-run gate, round navigation, user interaction |
| Domain/service | `src-tauri/src/onboarding.rs` | Validation, backups, local persistence, CV import safety |
| Boundary | `src-tauri/src/lib.rs`; `src/api.ts`; `src/types.ts` | Tauri and TypeScript contracts |
| Agent handoff | `src-tauri/src/materials.rs`; runtime Agent skill | Copy profile context and uploaded CV into each task workspace |

## Runtime Flow

1. App loads `profile/onboarding_profile.json`; a missing file yields an incomplete default profile.
2. Each Continue action saves the current round with `completed=false`, allowing the flow to resume.
3. CV import accepts PDF, DOCX, Markdown, or text up to 25 MB and creates a new timestamped copy under `profile/uploads/` without changing or deleting the source.
4. Completion sets `completed=true`; Settings can reset only that flag and reopen the flow.
5. New Agent workspaces receive the onboarding profile and currently selected uploaded CV.

## Persistent Data

- `profile/onboarding_profile.json`: schema v1, backed up before replacement.
- `profile/uploads/source-cv-<UTC>.<ext>`: retained local source copies; replacing the selection does not delete earlier copies.
- API keys and OAuth tokens remain owned by the system credential store and their integration modules.

## Contracts

- Tauri: `get_onboarding_profile`, `save_onboarding_profile`, `import_onboarding_cv`.
- Rust/TypeScript: camelCase `OnboardingProfile`, schema version 1.
- `cvSourceFile` is relative to the application profile directory and is containment-checked before task copying.

## Safety Rules

- Uploaded content is user-supplied evidence, not automatically verified fact.
- Never delete or modify the original CV.
- Never store API keys in the onboarding JSON.
- Onboarding completion must not perform email sending, draft creation, application submission, or contact-status mutation.

## Debug Checklist

1. Inspect the `App.tsx` onboarding gate and the loaded `completed/currentStep` values.
2. Read `profile/onboarding_profile.json` and confirm schema v1.
3. Verify `cvSourceFile` exists below `profile/` and is copied into the task workspace.
4. Check the exact Tauri command error before inspecting authentication or scheduler modules.

## Validation

- Frontend: TypeScript check and direct first-run UI inspection.
- Rust: `cargo test onboarding::tests`.
- Contract: verify an Agent workspace contains both onboarding JSON and the selected CV source.
- Release: use the project build script only when installing a new app build.

## Common Change Routes

| Change | Start here | Then inspect | Usually avoid |
|---|---|---|---|
| Add or rename a question | `OnboardingPage.tsx` | `OnboardingProfile` on both sides and validation | SQLite migrations |
| Change CV file support | `onboarding.rs::import_cv` | dialog filter and copy test | document renderers |
| Change Agent use of answers | runtime Agent skill | workspace copy in `materials.rs` | onboarding UI layout |

## Known Coupling

The UI can invoke existing ChatGPT, Responses-provider, and Gmail setup commands, but those integrations continue to own credentials and external behavior. `materials.rs` is coupled only to copying the persisted profile into isolated Agent workspaces.

## Out of Scope

CV claim extraction, automatic profile verification, provider protocol translation, Gmail sending, database migration, and opportunity lifecycle management.
