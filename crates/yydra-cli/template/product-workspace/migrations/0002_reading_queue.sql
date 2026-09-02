-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE reading_queue_entries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    source_url TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'queued',
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT reading_queue_title_valid CHECK (
        char_length(title) BETWEEN 1 AND 200
        AND title = btrim(title)
        AND title !~ '[[:cntrl:]]'
    ),
    CONSTRAINT reading_queue_source_url_valid CHECK (
        char_length(source_url) BETWEEN 1 AND 2048
        AND source_url = btrim(source_url)
        AND source_url ~ '^https?://[^[:space:]/?#]*[[:alnum:]][^[:space:]]*$'
    ),
    CONSTRAINT reading_queue_state_valid CHECK (state = 'queued')
);

CREATE INDEX reading_queue_entries_created_id_idx
ON reading_queue_entries (created_at ASC, id ASC);
