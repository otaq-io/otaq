-- A question can take several answers (`ask human --multiple`): the user ticks any of its options. A question asked
-- before keeps 0, one answer. `recommended_json` holds the recommended options when there are several (a JSON list
-- of 1-based numbers); `recommended` still holds the first. Never edit once committed.
ALTER TABLE human_questions ADD COLUMN multiple BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE human_questions ADD COLUMN recommended_json TEXT;
