-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE yydra_workspace_metadata (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO yydra_workspace_metadata (schema_name)
VALUES ('baseline');
