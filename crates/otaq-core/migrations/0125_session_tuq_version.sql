-- The tuq release an agent session's tool had when it started (tuq decision 102: a History row names the tuq a
-- session ran), read from the `metadata.tuq-release` stamp of the tool's installed tuq. NULL for a session from
-- before, or on a tool without tuq. Never edit once committed.
ALTER TABLE sessions ADD COLUMN tuq_version TEXT;
