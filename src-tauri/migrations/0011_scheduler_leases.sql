ALTER TABLE native_jobs ADD COLUMN active_key TEXT;
ALTER TABLE native_jobs ADD COLUMN timeout_seconds INTEGER NOT NULL DEFAULT 3600
    CHECK (timeout_seconds > 0);
ALTER TABLE native_jobs ADD COLUMN timeout_at TEXT;
ALTER TABLE native_jobs ADD COLUMN heartbeat_at TEXT;
ALTER TABLE native_jobs ADD COLUMN lease_owner TEXT;
ALTER TABLE native_jobs ADD COLUMN lease_expires_at TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_native_jobs_unique_active_key
    ON native_jobs(active_key)
    WHERE active_key IS NOT NULL AND status IN ('queued', 'running');

CREATE INDEX IF NOT EXISTS idx_native_jobs_expired_lease
    ON native_jobs(status, lease_expires_at)
    WHERE status = 'running';
