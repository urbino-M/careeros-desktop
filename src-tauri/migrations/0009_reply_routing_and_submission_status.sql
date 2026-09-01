ALTER TABLE contact_targets_v2 ADD COLUMN shelved_at TEXT;

ALTER TABLE contact_targets_v2 ADD COLUMN submission_status TEXT NOT NULL DEFAULT 'not_set'
    CHECK (submission_status IN ('not_set', 'portal_pending', 'submitted', 'not_required'));

CREATE INDEX IF NOT EXISTS idx_contact_targets_v2_shelved
    ON contact_targets_v2(shelved_at, updated_at DESC)
    WHERE archived_at IS NULL;
