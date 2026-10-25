-- M45 Helper questions, prompts and restarts (T-0538). Decisions 0133 (audit answers 3 and 5), 0048, 0030, 0119.
-- Never edit once committed.
--
-- A helper's `otaq ask human` goes to the agent in charge first (d0133 audit 3): the question is the ticket's
-- (`ticket_id`), asked by the helper (`helper_id`), and where it stands is `helper_state`: with the agent in charge
-- (the card stays Working), answered by it, or passed on to the user with its note (`pass_on_note`; only then does it
-- hold the ticket's Asks you, d0119). `helper_at` is when it left the agent in charge.
--
-- A helper's permission prompt goes straight to the user (d0011, d0016): `helpers.prompt` holds the tool's prompt
-- while it is open, `prompt_at` since when. `helpers.resume_prompt` is what a resumed helper is told first instead of
-- "Carry on with piece 3": the user's answer to its passed-on question given while the ticket was Interrupted.
ALTER TABLE human_questions ADD COLUMN helper_id TEXT REFERENCES helpers (id) ON DELETE CASCADE;
ALTER TABLE human_questions ADD COLUMN helper_state TEXT
    CHECK (helper_state IN ('with_in_charge', 'answered', 'passed_on'));
ALTER TABLE human_questions ADD COLUMN pass_on_note TEXT;
ALTER TABLE human_questions ADD COLUMN helper_at BIGINT;
CREATE INDEX human_questions_helper ON human_questions (helper_id) WHERE helper_id IS NOT NULL;

ALTER TABLE helpers ADD COLUMN prompt TEXT;
ALTER TABLE helpers ADD COLUMN prompt_at BIGINT;
ALTER TABLE helpers ADD COLUMN resume_prompt TEXT;
