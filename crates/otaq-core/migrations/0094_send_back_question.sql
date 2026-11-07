-- An agent may ask the user to send its ticket back to an earlier stage: the question names that stage, by its id and
-- by its name as it read when the agent asked. Both stay NULL on every other question. Never edit once committed.
ALTER TABLE human_questions ADD COLUMN send_back_stage_id TEXT;
ALTER TABLE human_questions ADD COLUMN send_back_stage TEXT;
