-- SPDX-License-Identifier: MIT OR Apache-2.0

DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM reading_queue_entries) THEN
        RAISE EXCEPTION 'Existing anonymous Reading Queue data requires an explicit account ownership migration';
    END IF;
END $$;
ALTER TABLE reading_queue_entries ADD COLUMN account_id UUID NOT NULL REFERENCES yydra_auth_accounts(id);
CREATE INDEX reading_queue_account_page ON reading_queue_entries (account_id, created_at, id);
DROP TABLE reading_progress;
CREATE TABLE reading_progress (
    account_id UUID PRIMARY KEY REFERENCES yydra_auth_accounts(id),
    completed_entries BIGINT NOT NULL CHECK (completed_entries >= 0)
);
