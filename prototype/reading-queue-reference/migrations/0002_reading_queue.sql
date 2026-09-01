CREATE TABLE reading_entries (
    id UUID PRIMARY KEY,
    title TEXT NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 240),
    source_url TEXT NOT NULL CHECK (source_url ~ '^https?://'),
    status TEXT NOT NULL CHECK (status IN ('queued', 'completed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX reading_entries_status_created_id_idx
    ON reading_entries (status, created_at DESC, id DESC);
