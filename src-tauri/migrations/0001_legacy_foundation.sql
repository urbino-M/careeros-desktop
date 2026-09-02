PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sources (
    id TEXT PRIMARY KEY,
    source_type TEXT NOT NULL,
    title TEXT NOT NULL,
    url TEXT,
    local_path TEXT,
    publisher TEXT,
    published_at TEXT,
    checked_at TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS pis (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    institution TEXT NOT NULL,
    department TEXT,
    country TEXT,
    region TEXT,
    email TEXT,
    homepage_url TEXT,
    lab_url TEXT,
    scholar_url TEXT,
    research_summary TEXT,
    research_keywords_json TEXT NOT NULL DEFAULT '[]',
    trajectory_json TEXT NOT NULL DEFAULT '[]',
    team_gap TEXT,
    funding_signal TEXT,
    source_checked_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS opportunities (
    id TEXT PRIMARY KEY,
    identity_key TEXT,
    pi_id TEXT REFERENCES pis(id) ON DELETE SET NULL,
    title TEXT NOT NULL,
    organization TEXT NOT NULL,
    department TEXT,
    country TEXT,
    region TEXT,
    opportunity_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'discovered',
    deadline TEXT,
    source_url TEXT,
    source_title TEXT,
    discovered_at TEXT,
    last_verified_at TEXT,
    summary TEXT,
    keywords_json TEXT NOT NULL DEFAULT '[]',
    fit_score REAL,
    priority TEXT NOT NULL DEFAULT 'unscored',
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS opportunity_aliases (
    alias_id TEXT PRIMARY KEY,
    opportunity_id TEXT NOT NULL REFERENCES opportunities(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS institutions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    city TEXT,
    country TEXT,
    region TEXT,
    qs_year INTEGER,
    qs_rank_display TEXT,
    qs_rank_numeric INTEGER,
    qs_scope TEXT NOT NULL DEFAULT 'World University Rankings',
    qs_status TEXT NOT NULL DEFAULT 'unknown',
    qs_source_url TEXT,
    qs_checked_at TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (qs_status IN ('ranked', 'banded', 'not_listed', 'not_applicable', 'unknown'))
);

CREATE TABLE IF NOT EXISTS opportunity_institutions (
    opportunity_id TEXT PRIMARY KEY REFERENCES opportunities(id) ON DELETE CASCADE,
    institution_id TEXT NOT NULL REFERENCES institutions(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS opportunity_actions (
    opportunity_id TEXT PRIMARY KEY REFERENCES opportunities(id) ON DELETE CASCADE,
    route TEXT NOT NULL DEFAULT 'unknown',
    application_url TEXT,
    contact_email TEXT,
    recommended_first_action TEXT,
    rationale TEXT,
    verified_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (route IN (
        'web_apply', 'email_first', 'email_application',
        'confirm_then_web', 'fellowship_eoi', 'unknown'
    ))
);

CREATE TABLE IF NOT EXISTS papers (
    id TEXT PRIMARY KEY,
    pi_id TEXT NOT NULL REFERENCES pis(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    year INTEGER,
    venue TEXT,
    doi TEXT,
    url TEXT,
    abstract TEXT,
    relevance_score REAL,
    relevance_notes TEXT,
    source_url TEXT,
    verified_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS fit_assessments (
    id TEXT PRIMARY KEY,
    opportunity_id TEXT REFERENCES opportunities(id) ON DELETE CASCADE,
    pi_id TEXT REFERENCES pis(id) ON DELETE CASCADE,
    research_theme REAL NOT NULL,
    methods REAL NOT NULL,
    evidence REAL NOT NULL,
    role_requirements REAL NOT NULL,
    logistics REAL NOT NULL,
    hiring_signal REAL NOT NULL,
    total REAL NOT NULL,
    recommendation TEXT NOT NULL,
    rationale TEXT NOT NULL,
    gaps TEXT,
    scoring_version TEXT NOT NULL DEFAULT 'v1',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (total >= 0 AND total <= 100)
);

CREATE TABLE IF NOT EXISTS applications (
    id TEXT PRIMARY KEY,
    opportunity_id TEXT REFERENCES opportunities(id) ON DELETE SET NULL,
    pi_id TEXT REFERENCES pis(id) ON DELETE SET NULL,
    status TEXT NOT NULL DEFAULT 'drafting',
    cv_path TEXT,
    email_path TEXT,
    cover_letter_path TEXT,
    research_pitch_path TEXT,
    contacted_at TEXT,
    follow_up_at TEXT,
    replied_at TEXT,
    outcome TEXT,
    notes TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS application_aliases (
    alias_id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS application_replies (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    sender TEXT,
    subject TEXT,
    body TEXT NOT NULL,
    body_sha256 TEXT NOT NULL,
    received_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(application_id, body_sha256)
);

CREATE TABLE IF NOT EXISTS application_contact_targets (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    source_job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    email TEXT,
    organization TEXT,
    title TEXT,
    fit_score REAL,
    priority INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'to_contact',
    source_url TEXT,
    result_path TEXT,
    contacted_at TEXT,
    replied_at TEXT,
    promoted_application_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(application_id, name),
    CHECK (status IN ('to_contact', 'contacted', 'replied', 'closed'))
);

CREATE TABLE IF NOT EXISTS application_claims (
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    claim_id TEXT NOT NULL,
    selected INTEGER NOT NULL DEFAULT 1,
    purpose TEXT,
    PRIMARY KEY (application_id, claim_id)
);

CREATE TABLE IF NOT EXISTS application_artifacts (
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en',
    path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (application_id, artifact_type, language)
);

CREATE TABLE IF NOT EXISTS gmail_drafts (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    gmail_draft_id TEXT NOT NULL UNIQUE,
    gmail_message_id TEXT,
    recipient TEXT NOT NULL,
    subject TEXT NOT NULL,
    cv_path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS artifact_revisions (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL,
    language TEXT NOT NULL,
    artifact_path TEXT NOT NULL,
    backup_path TEXT,
    editor TEXT NOT NULL,
    note TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (editor IN ('manual', 'codex', 'generator'))
);

CREATE TABLE IF NOT EXISTS revision_requests (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL,
    language TEXT NOT NULL,
    instructions TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    request_path TEXT,
    result_path TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (status IN ('queued', 'running', 'needs_review', 'completed', 'failed'))
);

CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    status TEXT NOT NULL DEFAULT 'queued',
    progress INTEGER NOT NULL DEFAULT 0,
    message TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    result_path TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    started_at TEXT,
    finished_at TEXT,
    CHECK (progress >= 0 AND progress <= 100)
);

CREATE TABLE IF NOT EXISTS job_controls (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    auto_start INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 2,
    attempts INTEGER NOT NULL DEFAULT 0,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    worker_pid INTEGER,
    heartbeat_at TEXT,
    command_text TEXT,
    log_path TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (auto_start IN (0, 1)),
    CHECK (cancel_requested IN (0, 1)),
    CHECK (max_retries >= 0),
    CHECK (attempts >= 0)
);

CREATE TABLE IF NOT EXISTS job_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    stage TEXT,
    status TEXT,
    progress INTEGER,
    message TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (progress IS NULL OR (progress >= 0 AND progress <= 100))
);

CREATE TABLE IF NOT EXISTS monitor_schedules (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    cadence_days INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    next_run_at TEXT,
    last_run_at TEXT,
    target_filter_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (cadence_days > 0),
    CHECK (enabled IN (0, 1))
);

CREATE TABLE IF NOT EXISTS opportunity_health_checks (
    id TEXT PRIMARY KEY,
    opportunity_id TEXT NOT NULL REFERENCES opportunities(id) ON DELETE CASCADE,
    checked_at TEXT NOT NULL,
    source_url TEXT,
    http_status INTEGER,
    health_status TEXT NOT NULL,
    deadline_status TEXT,
    evidence TEXT,
    error TEXT,
    next_check_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (health_status IN (
        'open', 'closed_signal', 'deadline_passed', 'ambiguous',
        'unreachable', 'no_source'
    ))
);

CREATE TABLE IF NOT EXISTS application_checklist_items (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    item_type TEXT NOT NULL,
    required INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL DEFAULT 'review',
    origin TEXT NOT NULL DEFAULT 'inferred',
    evidence TEXT,
    source_url TEXT,
    note TEXT,
    sort_order INTEGER NOT NULL DEFAULT 100,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(application_id, item_type),
    CHECK (required IN (0, 1)),
    CHECK (status IN ('missing', 'ready', 'review', 'not_required')),
    CHECK (origin IN ('source_extracted', 'inferred', 'manual'))
);

CREATE TABLE IF NOT EXISTS follow_up_reminders (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    reminder_type TEXT NOT NULL,
    due_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    draft_path TEXT,
    note TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(application_id, reminder_type, due_at),
    CHECK (status IN ('pending', 'due', 'drafted', 'completed', 'dismissed'))
);

CREATE TABLE IF NOT EXISTS feedback_events (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    signal TEXT NOT NULL,
    reason_code TEXT NOT NULL,
    note TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (signal IN ('positive', 'negative', 'neutral'))
);

CREATE TABLE IF NOT EXISTS preference_signals (
    signal_key TEXT PRIMARY KEY,
    category TEXT NOT NULL,
    positive_count INTEGER NOT NULL DEFAULT 0,
    negative_count INTEGER NOT NULL DEFAULT 0,
    score REAL NOT NULL DEFAULT 0,
    evidence_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (score >= -1 AND score <= 1)
);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    query TEXT NOT NULL,
    region TEXT,
    threshold REAL NOT NULL DEFAULT 75,
    threshold_operator TEXT NOT NULL DEFAULT 'gt',
    recency_days INTEGER NOT NULL DEFAULT 45,
    max_discovery INTEGER NOT NULL DEFAULT 20,
    max_deep_research INTEGER NOT NULL DEFAULT 8,
    max_materials INTEGER NOT NULL DEFAULT 5,
    ui_language TEXT NOT NULL DEFAULT 'zh',
    status TEXT NOT NULL DEFAULT 'queued',
    discovered_count INTEGER NOT NULL DEFAULT 0,
    researched_count INTEGER NOT NULL DEFAULT 0,
    retained_count INTEGER NOT NULL DEFAULT 0,
    material_count INTEGER NOT NULL DEFAULT 0,
    summary TEXT,
    error TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    started_at TEXT,
    finished_at TEXT,
    CHECK (threshold >= 0 AND threshold <= 100),
    CHECK (threshold_operator IN ('gt', 'gte')),
    CHECK (recency_days > 0),
    CHECK (max_discovery > 0),
    CHECK (max_deep_research > 0),
    CHECK (max_materials > 0)
);

CREATE TABLE IF NOT EXISTS run_candidates (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    candidate_key TEXT NOT NULL,
    title TEXT NOT NULL,
    organization TEXT NOT NULL,
    pi_name TEXT,
    country TEXT,
    opportunity_type TEXT,
    source_url TEXT,
    quick_score REAL,
    final_score REAL,
    decision TEXT NOT NULL DEFAULT 'discovered',
    reason TEXT,
    opportunity_id TEXT REFERENCES opportunities(id) ON DELETE SET NULL,
    application_id TEXT REFERENCES applications(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (run_id, candidate_key),
    CHECK (quick_score IS NULL OR (quick_score >= 0 AND quick_score <= 100)),
    CHECK (final_score IS NULL OR (final_score >= 0 AND final_score <= 100))
);

CREATE TABLE IF NOT EXISTS activity_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    detail TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_opportunities_fit
    ON opportunities(fit_score DESC);
CREATE INDEX IF NOT EXISTS idx_opportunities_status
    ON opportunities(status);
CREATE INDEX IF NOT EXISTS idx_papers_pi
    ON papers(pi_id);
CREATE INDEX IF NOT EXISTS idx_applications_status
    ON applications(status);
CREATE INDEX IF NOT EXISTS idx_application_artifacts
    ON application_artifacts(application_id);
CREATE INDEX IF NOT EXISTS idx_application_replies
    ON application_replies(application_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_application_contact_targets
    ON application_contact_targets(status, priority, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_artifact_revisions
    ON artifact_revisions(application_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_revision_requests
    ON revision_requests(application_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_institutions_qs
    ON institutions(qs_rank_numeric);
CREATE INDEX IF NOT EXISTS idx_jobs_status
    ON jobs(status);
CREATE INDEX IF NOT EXISTS idx_job_events_job
    ON job_events(job_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_job_controls_auto
    ON job_controls(auto_start, cancel_requested);
CREATE INDEX IF NOT EXISTS idx_monitor_schedules_due
    ON monitor_schedules(enabled, next_run_at);
CREATE INDEX IF NOT EXISTS idx_health_opportunity
    ON opportunity_health_checks(opportunity_id, checked_at DESC);
CREATE INDEX IF NOT EXISTS idx_checklist_application
    ON application_checklist_items(application_id, sort_order);
CREATE INDEX IF NOT EXISTS idx_followup_due
    ON follow_up_reminders(status, due_at);
CREATE INDEX IF NOT EXISTS idx_feedback_entity
    ON feedback_events(entity_type, entity_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_runs_created
    ON runs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_run_candidates_decision
    ON run_candidates(run_id, decision);
