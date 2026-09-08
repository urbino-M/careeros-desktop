-- User workflow decision, separate from recruitment availability and materials.
ALTER TABLE opportunities ADD COLUMN shelved_at TEXT;
