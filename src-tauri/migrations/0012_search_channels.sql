ALTER TABLE opportunities ADD COLUMN verification_status TEXT NOT NULL DEFAULT 'verified';
ALTER TABLE opportunities ADD COLUMN source_channel TEXT NOT NULL DEFAULT 'web_ats';
ALTER TABLE opportunities ADD COLUMN source_backend TEXT NOT NULL DEFAULT 'legacy';

ALTER TABLE native_source_evidence ADD COLUMN source_channel TEXT NOT NULL DEFAULT 'web_ats';
ALTER TABLE native_source_evidence ADD COLUMN backend TEXT NOT NULL DEFAULT 'legacy';

CREATE INDEX IF NOT EXISTS idx_opportunities_verification
    ON opportunities(opportunity_type, verification_status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_native_source_evidence_channel
    ON native_source_evidence(source_channel, backend, checked_at DESC);
