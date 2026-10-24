-- M40 Open questions hold Needs you (T-0489; ATT-01, ATT-02). Decisions 0119, 0042, 0079. Never edit once committed.
--
-- How a question to the user was closed: `box` (the answer box), `terminal` (the line the user sent with Enter in
-- that agent's terminal, or the Ask bar's Continue), `replaced` (the same agent asked a new question) or `closed`
-- (its ticket was closed, parked or delivered). `NULL` while it is open, and for a question its epic's lead
-- answered (`lead_state` says so). `answer` stays `NULL` for a replaced or closed question.
ALTER TABLE human_questions ADD COLUMN answered_via TEXT
    CHECK (answered_via IN ('box', 'terminal', 'replaced', 'closed'));

-- Closing, parking or delivering a ticket closes its open questions, whichever path moved it. A ticket parked before
-- it got a branch goes back to Backlog: its agent is gone too.
CREATE TRIGGER human_questions_close_with_ticket AFTER UPDATE OF state ON tickets
WHEN NEW.state <> OLD.state AND NEW.state IN ('backlog', 'parked', 'pr_open', 'merged', 'closed_not_merged')
BEGIN
    UPDATE human_questions
    SET answered_at = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER),
        answered_via = 'closed'
    WHERE ticket_id = NEW.id AND answered_at IS NULL;
END;
