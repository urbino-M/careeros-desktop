ALTER TABLE contact_targets_v2 ADD COLUMN status_version INTEGER NOT NULL DEFAULT 0;

-- A monotonic token also detects same-second edits and change-then-restore (ABA).
CREATE TRIGGER contact_status_version_changed
AFTER UPDATE OF status, shelved_at ON contact_targets_v2
BEGIN
    UPDATE contact_targets_v2 SET status_version = OLD.status_version + 1 WHERE id = NEW.id;
END;

CREATE INDEX native_jobs_opportunity_lookup
ON native_jobs(json_extract(CASE WHEN json_valid(payload_json) THEN payload_json ELSE '{}' END, '$.opportunityId'), created_at DESC)
WHERE job_type='full_search';
