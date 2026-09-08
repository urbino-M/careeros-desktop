-- Discovery is durable even when material generation is incomplete.
-- Existing records retain their prior ready state; new discoveries opt in to pending.
ALTER TABLE contact_targets_v2 ADD COLUMN material_status TEXT NOT NULL DEFAULT 'ready'
    CHECK(material_status IN ('pending','ready'));
ALTER TABLE contact_targets_v2 ADD COLUMN material_error TEXT;
