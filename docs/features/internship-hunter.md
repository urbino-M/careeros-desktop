# InternOS / Internship Hunter Handoff

## Product shape

InternOS is a parallel career workspace inside the PostdocOS desktop app. The
user selects `PostdocOS` or `InternOS` in the sidebar; the selected system
controls the brand, dashboard, navigation, task history, and application
filters. The two systems share the local SQLite database, Agent scheduler,
model settings, and safety boundaries, but they do not mix opportunity records
or workflow-specific status views.

## Scope

Internship Hunter discovers current industry internships from official sources,
checks hard eligibility conservatively, and saves review-only opportunity cards,
fit analyses, and application checklists. It does not tailor a resume, contact an
employer, or submit an application.

## Architecture

`AutomationPage.tsx` enqueues `internship_search`. The generic scheduler prepares
an isolated workspace, selects the internship runtime skill, runs Codex, and
passes `internship-search-results.json` to `workflows.rs`. The workflow validates
and deduplicates results before storing them in the existing
Opportunity/Application/contact-target data path. See `docs/ARCHITECTURE.md` for
the repository-wide ownership map.

## Source Ownership

| Area | Primary files | Responsibility |
|---|---|---|
| System switcher | `src/App.tsx`; `src/components/Shell.tsx` | Persisted PostdocOS / InternOS selection and system-scoped navigation |
| Entry point | `src/pages/AutomationPage.tsx` | Search request, threshold, and review navigation |
| Runtime contract | `src-tauri/resources/skills/internship-application-agent/SKILL.md`; `src-tauri/src/materials.rs` | Evidence rules and workspace contract |
| Domain import | `src-tauri/src/workflows.rs` | Result schema, validation, deduplication, and import |
| Presentation | `src/pages/DashboardPage.tsx`; `src/pages/ApplicationsPage.tsx`; `src/pages/ApplicationDetailPage.tsx` | System-scoped dashboard, application filters, opportunity and checklist views |

## Runtime Flow

1. The user supplies a role/location search brief and strict score threshold.
2. The Agent returns at most 20 official-source opportunities using the exact
   result contract.
3. The importer rejects inactive, ineligible, malformed, weak, or unsupported
   records and saves at most 10.
4. Saved cards enter `portal_pending`; the user reviews evidence and manually
   changes the submission marker.

## Persistent Data

The first slice reuses `opportunities`, `applications`, `contact_targets_v2`,
`contact_target_checklist`, `contact_target_artifacts`, `native_source_evidence`,
and `native_job_results`. `opportunities.opportunity_type=industry_internship`
is the track discriminator. No schema migration is introduced.

## Contracts

- Job type: `internship_search`.
- Output: `output/internship-search-results.json`, schema version 1.
- UI track: `TargetCard.careerTrack`, derived from `opportunity_type`.
- Backend list/dashboard filters: `careerTrack=postdoc|internship`; Internship
  status tabs use `submission_status`, while Postdoc tabs use contact status.
- Imported artifacts: bilingual `fit_analysis`; no CV or email artifacts.

## Safety Rules

- Official employer or official ATS evidence is required.
- The original PostdocOS candidate profile is not copied into internship search
  workspaces; candidate-specific scoring waits for Internship Hunter onboarding.
- Unknown candidate eligibility remains `uncertain`; it is never promoted by
  inference.
- The feature has no email-send or application-submit path.
- Existing postdoc records and job contracts remain unchanged.

## Debug Checklist

1. Inspect the `internship_search` row and payload in `native_jobs`.
2. Read `POSTDOCOS_TASK.json` and `output/internship-search-results.json` in the
   job workspace.
3. Check workflow validation warnings before persistence queries.
4. Confirm `opportunity_type=industry_internship` and the linked target result.

## Validation

- `npm run typecheck`
- `cd src-tauri && cargo test workflows::tests`

## Common Change Routes

| Change | Start here | Then inspect | Usually avoid |
|---|---|---|---|
| Search/output fields | `workflows.rs::result_contract` | Runtime skill and workflow tests | Scheduler lifecycle |
| Discovery UI | `AutomationPage.tsx` | `Ui.tsx`, TypeScript types | SQLite |
| Eligibility rule | Internship runtime skill | `validate_internship_opportunity` | Postdoc validation |

## Known Coupling

The first slice uses a synthetic `Application portal` contact target so existing
job-result routing and application views remain usable. Track-specific pipeline
storage should replace this compatibility seam before interview/offer stages are
added. The current system selector is intentionally small and local; account-level
preferences and separate onboarding can be added without changing the shared
storage boundary.

## Out of Scope

Fresh-database bootstrap, resume tailoring, referrals, recruiter outreach,
automatic submissions, interviews, offers, and a generalized CareerOS schema.
