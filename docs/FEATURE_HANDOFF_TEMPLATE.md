# <Feature> Handoff

Use this template only for a complex, long-lived subsystem. Save an instantiated handoff as `docs/features/<kebab-case-feature>.md`; do not create one for a simple component. Delete guidance that is not relevant and keep the resulting handoff operational and concise.

## Scope

Describe what the subsystem owns, who uses it, and what it explicitly does not own.

## Architecture

Summarize the data flow and main call chain. Link to Source Ownership in `docs/ARCHITECTURE.md`; do not duplicate the repository-wide map.

## Source Ownership

| Area | Primary files | Responsibility |
|---|---|---|
| Entry point | `<real path>` | What begins the flow |
| Domain/service | `<real path>` | Rules and orchestration owned here |
| Persistence/integration | `<real path>` | Stored or external effects |

## Runtime Flow

1. Describe the trigger.
2. Describe boundary crossings and validation.
3. Describe persistence or external effects.
4. Describe completion, review, cancellation, and failure behavior where relevant.

## Persistent Data

List owned tables, files, directories, secrets, identifiers, retention, and compatibility expectations. Write `None` if the feature is not persistent.

## Contracts

List applicable Tauri commands, Rust request/result types, serde semantics, TypeScript types, file formats, and external API contracts.

## Safety Rules

- State the invariants that must not be broken.
- State irreversible or externally visible effects.
- State ownership/path/authentication boundaries.

## Debug Checklist

1. Start at the owning entry point.
2. Inspect the nearest state/record/event.
3. Confirm contract and path assumptions.
4. Inspect subsystem logs or targeted errors.
5. Expand into coupled modules only with evidence.

## Validation

List the smallest targeted tests, fixtures, or manual checks that prove this subsystem. Separate local, contract, infrastructure, and release checks.

## Common Change Routes

| Change | Start here | Then inspect | Usually avoid |
|---|---|---|---|
| `<common request>` | `<symbol/path>` | `<contract/test>` | `<unrelated boundary>` |

## Known Coupling

List only real coupling: what crosses the boundary, why it exists, and which side owns the contract.

## Out of Scope

List adjacent responsibilities that future maintainers should route elsewhere.
