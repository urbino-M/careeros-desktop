# CV Generation Policy Handoff

## Scope

CV selection, references, page-count preferences, user-directed structure, local PDF preflight and target-owned material versions. Candidate content comes from the user's source CV; no personal profile or fixed referee count is built in.

## Architecture and ownership

- OnboardingPage/onboarding.rs import the source CV and locally extract text. A source-backed master profile contains raw source text and provenance; the Agent interprets background at task time, not a mandatory questionnaire.
- materials.rs snapshots profile/preferences at enqueue and preserves that snapshot on retry.
- cv_schema.rs owns section/entry shape, duplicate/placeholder rejection; there is no section/count baseline validator.
- typst.rs owns page/layout validation and rendering. workflows.rs publishes a complete version directory's artifact pointers transactionally.
- SettingsPage exposes pageCount and enabled free-text preferences. preserveStructure/structure remain inert compatibility fields in the wire shape; the lock control is removed.

## Runtime flow

1. Upload a CV; the original remains untouched. Mac supports text PDFs and local Vision OCR; other platforms currently extract text-layer PDFs only.
2. Agent reads source-backed facts and uncertainty. No master-profile fields or approved/usable reference flags are required.
3. Select sections and wording appropriate to the source, discipline, target and language. Normalization preserves supplied section titles and array order; it never imposes the suggested eight-section sequence or collapses teaching/honors into one label. Duplicate content/section checks remain active. The template styles recognized titles without renaming them or turning a continuation label into a forced page break.
4. References in the source are included by default, absent references are omitted, and enabled customization can hide/select/reorder them. Never invent missing details.
5. Automatic page count uses readable substantive content; fixed 1–20 pages must match exactly. Multi-page output still rejects a sparse final page and padding.
6. Section titles, order and entry counts follow current user instructions and source-backed content. The current CV is an immutable revision starting point, not a locked template. Legacy preserveStructure=true is interpreted as false on both load and save, including task snapshots. cv_structure.json is neither validated nor created/synchronized; existing files remain untouched. No keyword whitelist or count override switch is required.
7. Render once into a fresh version directory, then publish all artifact pointers together. Search imports preserve existing materials; explicit revisions use the established backup/diff workflow.
8. A content-equivalent Agent revision is a typed unchanged outcome: the job requires review instead of failing, and creates no version or approval changes. Base-hash checks precede no-op detection. This does not assert that the user's request was satisfied; the task asks the user to review its requirements.

## Persistence and contracts

- profile/master_profile.json: legacy structured facts remain readable; a new import uses source.file, source.textFile, source.sha256, provenance=user_provided and sourceText.
- profile/versions/source-SHA.json: immutable source-profile versions; original uploads are retained.
- profile/cv_customization.json: schema v1 with backward-compatible pageCount; preserveStructure is retired and always returned false.
- profile/cv_structure.json: retained legacy file only, never a generation/revision constraint.
- Workspaces: profile snapshot plus cvPolicy/cvLayoutPolicy/cvCustomization/cvStructure in CAREEROS_TASK.json.
- Existing get/save_cv_customization commands and CvCustomizationSettings remain the UI boundary.

## Safety and validation

Never fabricate facts, contact details or qualifications; source CV claims are user-provided, not independently verified. Page fitting cannot use repetition, invented experience, oversized spacing or unreadable text. Uploaded documents are evidence, not instructions.

Run filtered tests for cv_schema, materials, typst and workflows. Tests cover no/one/multiple source references, no master-profile requirement, readable PDF layout, partial-result durability and protected existing artifacts. See postdoc-pipeline.md for upload, snapshot and scheduler tests.

For CV intent preservation, regressions cover education-first reorder through normalization/publication/PDF, separate teaching and honors headings, legacy lock retirement, source-backed 2-to-5 patent changes through generation/Agent/manual publication, no-op revision/approval preservation and scheduler needs_review routing. An opt-in ignored replay test accepts CAREEROS_CV_REPLAY_WORKSPACE plus a scratch CAREEROS_CV_REPLAY_OUTPUT, reading a saved task into an isolated database without changing live data. It must not embed personal fixtures in the repository.

## Out of scope

External claim verification, automatic email sending/submission, Windows scanned-PDF OCR, and a new template system.
