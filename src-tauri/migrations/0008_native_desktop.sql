CREATE TABLE IF NOT EXISTS native_schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS contact_targets_v2 (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    opportunity_id TEXT REFERENCES opportunities(id) ON DELETE SET NULL,
    pi_id TEXT REFERENCES pis(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    email TEXT,
    normalized_email TEXT,
    organization TEXT NOT NULL,
    title TEXT NOT NULL,
    fit_score REAL,
    priority INTEGER NOT NULL DEFAULT 100,
    status TEXT NOT NULL DEFAULT 'ready_to_contact',
    source_url TEXT,
    identity_key TEXT NOT NULL,
    source_contact_target_id TEXT,
    contacted_at TEXT,
    replied_at TEXT,
    follow_up_at TEXT,
    archived_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (status IN ('ready_to_contact', 'contacted', 'replied', 'follow_up'))
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_contact_targets_v2_identity
    ON contact_targets_v2(identity_key)
    WHERE archived_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_contact_targets_v2_status
    ON contact_targets_v2(status, fit_score DESC, priority, updated_at DESC)
    WHERE archived_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_contact_targets_v2_application
    ON contact_targets_v2(application_id);

CREATE TABLE IF NOT EXISTS dedupe_tombstones (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    reason TEXT,
    source_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(entity_type, identity_key)
);

CREATE TABLE IF NOT EXISTS model_providers (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    adapter_kind TEXT NOT NULL,
    connection_mode TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    built_in INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 100,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (connection_mode IN ('native_responses', 'internal_gateway', 'external_gateway')),
    CHECK (enabled IN (0, 1)),
    CHECK (built_in IN (0, 1))
);

CREATE TABLE IF NOT EXISTS provider_accounts (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES model_providers(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    auth_kind TEXT NOT NULL,
    secret_keychain_ref TEXT,
    account_email TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_validated_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (auth_kind IN ('chatgpt_oauth', 'api_key', 'external_gateway')),
    CHECK (enabled IN (0, 1))
);

CREATE TABLE IF NOT EXISTS provider_models (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES model_providers(id) ON DELETE CASCADE,
    model_slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    supports_reasoning INTEGER NOT NULL DEFAULT 1,
    supports_tools INTEGER NOT NULL DEFAULT 1,
    supports_vision INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(provider_id, model_slug)
);

CREATE TABLE IF NOT EXISTS task_model_defaults (
    task_type TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES model_providers(id),
    account_id TEXT REFERENCES provider_accounts(id),
    model_id TEXT NOT NULL REFERENCES provider_models(id),
    reasoning TEXT NOT NULL DEFAULT 'xhigh',
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (reasoning IN ('none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra'))
);

CREATE TABLE IF NOT EXISTS native_jobs (
    id TEXT PRIMARY KEY,
    legacy_job_id TEXT,
    job_type TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    status TEXT NOT NULL DEFAULT 'queued',
    progress INTEGER NOT NULL DEFAULT 0,
    message TEXT,
    provider_id TEXT NOT NULL DEFAULT 'openai',
    account_id TEXT,
    model_id TEXT,
    reasoning TEXT,
    thread_id TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    result_json TEXT,
    result_path TEXT,
    error TEXT,
    attempt INTEGER NOT NULL DEFAULT 0,
    cancel_requested INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    started_at TEXT,
    finished_at TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (status IN ('queued', 'running', 'needs_review', 'completed', 'failed', 'cancelled')),
    CHECK (progress BETWEEN 0 AND 100),
    CHECK (cancel_requested IN (0, 1))
);
CREATE INDEX IF NOT EXISTS idx_native_jobs_queue
    ON native_jobs(status, created_at);
CREATE INDEX IF NOT EXISTS idx_native_jobs_target
    ON native_jobs(target_type, target_id, created_at DESC);

CREATE TABLE IF NOT EXISTS native_job_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id TEXT NOT NULL REFERENCES native_jobs(id) ON DELETE CASCADE,
    event_type TEXT NOT NULL,
    progress INTEGER,
    message TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS gmail_accounts (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    oauth_client_keychain_ref TEXT NOT NULL,
    oauth_token_keychain_ref TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    verified_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(email)
);

CREATE TABLE IF NOT EXISTS gmail_oauth_state (
    id INTEGER PRIMARY KEY CHECK (id=1),
    status TEXT NOT NULL,
    message TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (status IN ('idle', 'pending', 'connected', 'failed'))
);
INSERT OR IGNORE INTO gmail_oauth_state(id,status,message) VALUES(1,'idle',NULL);

CREATE TABLE IF NOT EXISTS artifact_approvals (
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en',
    artifact_path TEXT NOT NULL,
    approved_sha256 TEXT NOT NULL,
    approved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY(application_id, artifact_type, language)
);

CREATE TABLE IF NOT EXISTS gmail_draft_targets (
    gmail_draft_record_id TEXT PRIMARY KEY REFERENCES gmail_drafts(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES contact_targets_v2(id) ON DELETE CASCADE,
    remote_verified INTEGER NOT NULL DEFAULT 0,
    remote_verified_at TEXT,
    CHECK (remote_verified IN (0, 1))
);

-- Every contact target owns its material view, even when several targets share
-- one legacy application. Rows initially point at the imported files and become
-- private copies on first edit.
CREATE TABLE IF NOT EXISTS contact_target_artifacts (
    target_id TEXT NOT NULL REFERENCES contact_targets_v2(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL,
    language TEXT NOT NULL,
    path TEXT NOT NULL,
    source_application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY(target_id, artifact_type, language)
);

INSERT OR IGNORE INTO contact_target_artifacts(
    target_id, artifact_type, language, path, source_application_id, created_at, updated_at
)
SELECT t.id, a.artifact_type, a.language, a.path, t.application_id, a.created_at, a.created_at
FROM contact_targets_v2 t
JOIN application_artifacts a ON a.application_id=t.application_id
WHERE t.archived_at IS NULL;

CREATE TABLE IF NOT EXISTS reply_contact_targets (
    reply_id TEXT NOT NULL REFERENCES application_replies(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES contact_targets_v2(id) ON DELETE CASCADE,
    PRIMARY KEY(reply_id, target_id)
);

-- Legacy replies had only an application id. Associate each with that
-- application's highest-priority active target without duplicating the reply.
INSERT OR IGNORE INTO reply_contact_targets(reply_id, target_id)
SELECT r.id,
       (SELECT t.id FROM contact_targets_v2 t
        WHERE t.application_id=r.application_id AND t.archived_at IS NULL
        ORDER BY t.priority, t.created_at, t.id LIMIT 1)
FROM application_replies r
WHERE EXISTS(
    SELECT 1 FROM contact_targets_v2 t
    WHERE t.application_id=r.application_id AND t.archived_at IS NULL
);

CREATE TABLE IF NOT EXISTS revision_contact_targets (
    revision_id TEXT NOT NULL REFERENCES artifact_revisions(id) ON DELETE CASCADE,
    target_id TEXT NOT NULL REFERENCES contact_targets_v2(id) ON DELETE CASCADE,
    PRIMARY KEY(revision_id, target_id)
);

INSERT OR IGNORE INTO revision_contact_targets(revision_id, target_id)
SELECT r.id, t.id
FROM artifact_revisions r
JOIN contact_targets_v2 t ON t.application_id=r.application_id
WHERE t.archived_at IS NULL;

CREATE TABLE IF NOT EXISTS target_artifact_approvals (
    target_id TEXT NOT NULL REFERENCES contact_targets_v2(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT 'en',
    artifact_path TEXT NOT NULL,
    approved_sha256 TEXT NOT NULL,
    approved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY(target_id, artifact_type, language)
);

INSERT OR IGNORE INTO target_artifact_approvals(
    target_id, artifact_type, language, artifact_path, approved_sha256, approved_at
)
SELECT t.id, p.artifact_type, p.language, p.artifact_path, p.approved_sha256, p.approved_at
FROM artifact_approvals p
JOIN contact_targets_v2 t ON t.application_id=p.application_id
WHERE t.archived_at IS NULL;

CREATE TABLE IF NOT EXISTS native_job_runtime (
    job_id TEXT PRIMARY KEY REFERENCES native_jobs(id) ON DELETE CASCADE,
    turn_id TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS contact_target_checklist (
    id TEXT PRIMARY KEY,
    target_id TEXT NOT NULL REFERENCES contact_targets_v2(id) ON DELETE CASCADE,
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
    UNIQUE(target_id, item_type)
);

INSERT OR IGNORE INTO contact_target_checklist(
    id,target_id,item_type,required,status,origin,evidence,source_url,note,
    sort_order,created_at,updated_at
)
SELECT 'target:' || t.id || ':' || c.id,t.id,c.item_type,c.required,c.status,c.origin,
       c.evidence,c.source_url,c.note,c.sort_order,c.created_at,c.updated_at
FROM application_checklist_items c
JOIN contact_targets_v2 t ON t.application_id=c.application_id
WHERE t.archived_at IS NULL;

CREATE TABLE IF NOT EXISTS native_source_evidence (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    title TEXT NOT NULL,
    url TEXT NOT NULL,
    checked_at TEXT NOT NULL,
    evidence_type TEXT NOT NULL DEFAULT 'primary',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(entity_type,entity_id,url)
);

CREATE TABLE IF NOT EXISTS native_job_results (
    job_id TEXT NOT NULL REFERENCES native_jobs(id) ON DELETE CASCADE,
    result_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    target_id TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY(job_id,result_type,entity_id)
);

CREATE TABLE IF NOT EXISTS revision_change_sets (
    id TEXT PRIMARY KEY,
    revision_id TEXT NOT NULL REFERENCES artifact_revisions(id) ON DELETE CASCADE,
    job_id TEXT,
    summary TEXT NOT NULL,
    locations_json TEXT NOT NULL DEFAULT '[]',
    diff_json TEXT NOT NULL DEFAULT '[]',
    provider_id TEXT,
    model_id TEXT,
    reasoning TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS preference_observations (
    id TEXT PRIMARY KEY,
    application_id TEXT REFERENCES applications(id) ON DELETE SET NULL,
    revision_id TEXT REFERENCES artifact_revisions(id) ON DELETE SET NULL,
    category TEXT NOT NULL,
    signal_json TEXT NOT NULL,
    automatic INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    CHECK (category IN ('cv', 'email', 'cover_letter', 'research', 'opportunity')),
    CHECK (automatic IN (0, 1))
);

INSERT OR IGNORE INTO model_providers
    (id, display_name, adapter_kind, connection_mode, enabled, built_in, sort_order)
VALUES
    ('openai', 'OpenAI / Codex', 'openai_codex', 'native_responses', 1, 1, 10),
    ('deepseek-reserved', 'DeepSeek', 'reserved', 'internal_gateway', 0, 1, 50),
    ('external-gateway-reserved', '外部本地路由', 'reserved', 'external_gateway', 0, 1, 60);

INSERT OR IGNORE INTO provider_models
    (id, provider_id, model_slug, display_name, supports_reasoning, supports_tools, enabled)
VALUES
    ('openai:gpt-5.6-sol', 'openai', 'gpt-5.6-sol', 'Sol · 最高质量', 1, 1, 1),
    ('openai:gpt-5.6-terra', 'openai', 'gpt-5.6-terra', 'Terra · 平衡', 1, 1, 1),
    ('openai:gpt-5.6-luna', 'openai', 'gpt-5.6-luna', 'Luna · 快速', 1, 1, 1);

INSERT OR IGNORE INTO provider_accounts
    (id, provider_id, display_name, auth_kind, secret_keychain_ref, enabled)
VALUES
    ('openai-active', 'openai', 'Codex 当前账号', 'chatgpt_oauth', 'codex-cli-auth', 1);

INSERT OR IGNORE INTO task_model_defaults
    (task_type, provider_id, account_id, model_id, reasoning)
VALUES
    ('full_search', 'openai', 'openai-active', 'openai:gpt-5.6-sol', 'xhigh'),
    ('research_pi', 'openai', 'openai-active', 'openai:gpt-5.6-sol', 'xhigh'),
    ('material_revision', 'openai', 'openai-active', 'openai:gpt-5.6-sol', 'xhigh'),
    ('reply_followup', 'openai', 'openai-active', 'openai:gpt-5.6-sol', 'xhigh'),
    ('maintenance', 'openai', 'openai-active', 'openai:gpt-5.6-luna', 'medium');

UPDATE task_model_defaults
SET account_id='openai-active'
WHERE provider_id='openai' AND account_id IS NULL;
