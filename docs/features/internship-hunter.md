# Internship Hunter Handoff

## Product shape

Internship Hunter is the independent Internship track inside the unified
CareerOS desktop workspace. Its list and strategy views share the CareerOS
shell, but the Internship search profile and submission safeguards are separate
from Postdoc records.

## Scope

Internship Hunter discovers current industry internships through official Web /
ATS, Exa, RSS, LinkedIn, Facebook, and Twitter / X. It checks hard eligibility
conservatively, preserves source evidence, and saves review-only opportunity
cards, fit analyses, and application checklists. It does not tailor a resume,
contact an employer, or submit an application.

## Architecture

`AutomationPage.tsx` enqueues `internship_search`. The scheduler prepares an
isolated workspace, runs the native channel adapters before Codex, writes
`input/channel-results.json`, selects the Internship runtime skill, and passes
`output/internship-search-results.json` to `workflows.rs`. The workflow validates,
deduplicates, classifies verification, and imports results through the existing
Opportunity/Application/contact-target path.

The native channel boundary lives in `src-tauri/src/search_channels.rs`:

```text
doctor() -> ChannelHealth
search(request) -> RawChannelResults
normalize(raw) -> ChannelResults
```

Each adapter has its own health check and warning path. Available channels run
in parallel; one channel failure does not discard other results. Twitter uses
OpenCLI first and `twitter-cli` as a fallback. Exa and LinkedIn use `mcporter`;
RSS is fetched and parsed by CareerOS; Facebook uses OpenCLI; official Web /
ATS evidence remains the Codex web-search route.

## Source ownership

| Area | Primary files | Responsibility |
|---|---|---|
| Search channels | `src-tauri/src/search_channels.rs`; `src-tauri/src/models.rs` | Capability checks, adapters, preferred/fallback backends, parallel search, normalization, and provenance |
| Internship profile | `src-tauri/src/internship.rs`; `src/components/InternshipPlanningPanel.tsx` | Independent `profile/internship.json`, optional profile-local CV, RSS feeds, and strategy UI |
| Setup/auth commands | `src-tauri/src/lib.rs`; `src/api.ts`; `src/types.ts` | One-click user-level installation, channel health, and browser login guidance without credential handling |
| Runtime contract | `src-tauri/resources/skills/internship-application-agent/SKILL.md`; `src-tauri/src/materials.rs` | Channel-result input, evidence rules, workspace files, and output contract |
| Domain import | `src-tauri/src/workflows.rs` | Result validation, cross-channel deduplication, verification classification, checklist and evidence persistence |
| Persistence | `src-tauri/migrations/0012_search_channels.sql`; `src-tauri/src/migration.rs`; `src-tauri/src/db.rs` | Verification status, source channel/backend, filtered list/detail queries, and unverified submission protection |
| Presentation | `src/pages/DashboardPage.tsx`; `src/pages/ApplicationsPage.tsx`; `src/pages/ApplicationDetailPage.tsx`; `src/components/Ui.tsx` | Channel status, verification badges, provenance, pending-verification filter, and disabled submission control |

## Runtime flow

1. The user optionally saves the independent Internship profile. Empty fields do
   not block searching; the Agent uses `eligibilityStatus=uncertain` when the
   available profile evidence cannot establish a hard requirement.
2. Before Codex starts, `search_channels::run` checks all six channels and runs
   available adapters concurrently. It writes normalized results, channel
   health, timestamps, backend identifiers, and warnings to
   `input/channel-results.json`.
3. The Agent combines the normalized results and may perform direct official
   Web / ATS verification. It returns at most 20 discovered opportunities with
   the exact result contract.
4. The importer deduplicates by canonical URL, stable identifier, or a
   conservative company-title-location key, then saves at most 10.
5. An opportunity is `verified` only when its sources include a `web_ats` source
   with `evidenceType=primary`. Opportunities supported only by Exa, RSS,
   LinkedIn, Facebook, or Twitter / X are `unverified` and remain separate from
   direct-application/submitted states.

## Setup and login

The settings page exposes `get_search_capabilities`,
`setup_search_capabilities`, and `begin_search_channel_auth` through the
“管理信息搜索渠道” section. A user action on “一键启用” directly installs
OpenCLI, mcporter, `uv`, or `twitter-cli` into user-level locations and writes
only user-level mcporter configuration; it never uses `sudo` or writes the
project directory. The strategy page shows only a compact channel summary and
links back to this settings section.

CareerOS does not enter credentials and never reads, prints, or stores
passwords, browser cookies, or tokens. Facebook, LinkedIn, and Twitter / X may
reuse the user's existing browser session. Twitter may also be configured by
the user through the upstream CLI's local authentication. When a session is
missing, clicking “连接渠道” opens the centralized browser login guide and
CareerOS polls the safe channel health result for up to 60 seconds. If the
upstream cannot expose a verifiable authenticated state, the UI asks the user
to click “立即检查” instead of claiming that login succeeded.

## Persistent data and contracts

- Profile: `profile/internship.json`; optional CV paths must remain inside the
  Internship profile directory and are never copied from Postdoc's
  `master_profile.json`.
- Channel input: `input/channel-results.json`.
- Agent output: `output/internship-search-results.json`, schema version 1.
- Track discriminator: `opportunities.opportunity_type=industry_internship`.
- Persisted provenance: `opportunities.verification_status`,
  `opportunities.source_channel`, `opportunities.source_backend`, and the
  matching fields in `native_source_evidence`.
- Internship list filters use verification plus submission status; Postdoc
  filters continue to use contact status.
- No automatic re-verification or promotion of an unverified opportunity is
  scheduled. The user can inspect the source and decide what to do manually.

## Safety rules

- Official employer or official ATS evidence is required for `verified`; a
  secondary source alone is never upgraded by inference.
- Unknown candidate eligibility remains `uncertain`, not favorable by default.
- The database rejects attempts to mark an unverified opportunity as
  `portal_pending` or `submitted`, and the detail UI disables that control.
- No channel adapter receives credentials from CareerOS; subprocess output is
  parsed into allow-listed normalized fields and stderr is not surfaced.
- The feature has no email-send, message-send, automatic-submit, or automatic-
  login path.
- The feature does not include TikTok, Xiaohongshu, Instagram, Reddit,
  Bilibili, or YouTube.

## Debug checklist

1. Inspect the `internship_search` row and payload in `native_jobs`.
2. Read `CAREEROS_TASK.json`, `input/channel-results.json`, and
   `output/internship-search-results.json` in the job workspace.
3. Check channel warnings and workflow validation errors before inspecting
   persistence rows.
4. Confirm `opportunity_type=industry_internship`, verification/source columns,
   and the linked target result.
5. For a submission-state error, verify the opportunity's
   `verification_status` before debugging the UI.

## Validation

- `npm run typecheck`
- `npm test -- src/pages/ApplicationDetailPage.test.ts`
- `cd src-tauri && cargo test search_channels::tests`
- `cd src-tauri && cargo test internship::tests`
- `cd src-tauri && cargo test workflows::tests`
- `cd src-tauri && cargo test db::tests`
- `cd src-tauri && cargo test scheduler::tests`
- `cd src-tauri && cargo test migration::tests`
- `git diff --check`

## Out of scope

Fresh application-stage modeling, resume tailoring, recruiter outreach, direct
submission, automatic login, automatic re-verification, and additional social
channels remain out of scope.
