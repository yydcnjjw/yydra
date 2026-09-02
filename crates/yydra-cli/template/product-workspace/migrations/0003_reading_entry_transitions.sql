-- SPDX-License-Identifier: MIT OR Apache-2.0

ALTER TABLE reading_queue_entries
DROP CONSTRAINT reading_queue_state_valid;

ALTER TABLE reading_queue_entries
ADD CONSTRAINT reading_queue_state_valid
CHECK (state IN ('queued', 'completed'));
