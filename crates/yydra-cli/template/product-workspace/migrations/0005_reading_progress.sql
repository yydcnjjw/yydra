-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE TABLE reading_progress (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    completed_entries BIGINT NOT NULL CHECK (completed_entries >= 0)
);

INSERT INTO reading_progress (singleton, completed_entries)
SELECT TRUE, COUNT(*)
FROM reading_queue_entries
WHERE state = 'completed';
