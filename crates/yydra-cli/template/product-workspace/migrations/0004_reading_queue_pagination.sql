-- SPDX-License-Identifier: MIT OR Apache-2.0

CREATE INDEX reading_queue_entries_state_created_id_idx
ON reading_queue_entries (state, created_at ASC, id ASC);
