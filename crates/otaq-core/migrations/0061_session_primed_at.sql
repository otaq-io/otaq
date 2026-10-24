-- M40 A prime before the stage run opens (T-0493). Decisions 0109, 0079. Never edit once committed.
--
-- `primed_at`: when the session ran `prime` while its ticket had no open stage run yet (Unix ms). A new agent can read
-- its prime before the start sets the ticket Working and the trigger opens its run; the read gate (CMT-08) counts it
-- for the run the session opens first, unless Otaq handed it a new comment since.
ALTER TABLE sessions ADD COLUMN primed_at BIGINT;
