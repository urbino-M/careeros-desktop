ALTER TABLE model_providers ADD COLUMN base_url TEXT;
ALTER TABLE model_providers ADD COLUMN wire_api TEXT NOT NULL DEFAULT 'responses'
    CHECK (wire_api = 'responses');
ALTER TABLE model_providers ADD COLUMN last_validated_at TEXT;
ALTER TABLE model_providers ADD COLUMN validation_message TEXT;

