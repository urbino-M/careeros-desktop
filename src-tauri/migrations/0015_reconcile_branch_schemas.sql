-- Column additions are guarded by physical-schema checks in migration.rs.
-- Retain either historical v12 name and all existing verification values.
CREATE INDEX IF NOT EXISTS idx_opportunities_verification
    ON opportunities(opportunity_type, verification_status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_native_source_evidence_channel
    ON native_source_evidence(source_channel, backend, checked_at DESC);
INSERT INTO native_schema_migrations(version, name)
    VALUES(15, 'reconcile-postdoc-and-internship-branches');
