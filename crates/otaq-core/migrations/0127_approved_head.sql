-- The commit the user approved the ticket's work at, in the last human check they passed (tuq decision 116): a later
-- agent check judges only what changed since. The stage approved at and when. NULL before any approval, and again
-- after a Reopen. Never edit once committed.
ALTER TABLE tickets ADD COLUMN approved_sha TEXT;
ALTER TABLE tickets ADD COLUMN approved_stage_id TEXT;
ALTER TABLE tickets ADD COLUMN approved_at BIGINT;
