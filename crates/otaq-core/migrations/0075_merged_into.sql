-- A user merges one ticket into another ticket's branch. Never edit once committed.
--
-- `merged_into`: one row per ticket squashed into another open ticket's branch (its host), with the squash commit on
-- the host's branch and when. The ticket waits as `merged_into` in its Delivery stage until its host lands or is
-- closed; the row stays as the record of what went into the host. A ticket is merged into at most one host at a time.
--
-- The ticket's `merged_into` state is a Delivery stage's: the stage run that follows the ticket into it is a
-- `delivery` run, and its open questions close as they do for `pr_open`. Both triggers are as 0074 recreated them,
-- with that state added.
CREATE TABLE merged_into (
    ticket_id  TEXT   NOT NULL PRIMARY KEY REFERENCES tickets (id) ON DELETE CASCADE,
    into_id    TEXT   NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,  -- the host
    commit_sha TEXT   NOT NULL,                                            -- the squash commit on the host's branch
    merged_at  BIGINT NOT NULL
);
CREATE INDEX merged_into_host ON merged_into (into_id);

DROP TRIGGER stage_runs_follow_tickets;
CREATE TRIGGER stage_runs_follow_tickets AFTER UPDATE OF stage_id, state ON tickets
WHEN NEW.stage_id IS NOT OLD.stage_id
  OR (NEW.stage_id IS NOT NULL
      AND OLD.state IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted')
      AND NEW.state NOT IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted'))
BEGIN
    UPDATE stage_runs
    SET ended_at = CAST(unixepoch('subsec') * 1000 AS INTEGER),
        result = COALESCE(result, CASE WHEN NEW.state = 'merged' OR NEW.stage_id IS NOT NULL THEN 'done'
                                       ELSE 'cancelled' END)
    WHERE ticket_id = NEW.id AND ended_at IS NULL;
    INSERT INTO stage_runs (id, ticket_id, stage_id, kind, session_id, agent_id, agent, model, effort, started_at)
    SELECT lower(hex(randomblob(16))), NEW.id, NEW.stage_id, k.kind, s.id, s.agent_id, s.agent_name,
           COALESCE(s.model_ran, s.model), s.effort, CAST(unixepoch('subsec') * 1000 AS INTEGER)
    FROM (SELECT CASE
              WHEN NEW.state IN ('validating', 'validator_needs_you', 'validation_failed', 'validation_no_result')
                OR NEW.interrupted_from IN ('validating', 'validator_needs_you') THEN 'check'
              WHEN NEW.state = 'ready_for_review' THEN 'you'
              WHEN NEW.state IN ('to_deliver', 'pr_open', 'merged_into') THEN 'delivery'
              ELSE 'agent'
          END AS kind) k
    LEFT JOIN sessions s ON s.id = (
        SELECT id FROM sessions
        WHERE ticket_id = NEW.id AND ended_at IS NULL
          AND kind = CASE k.kind WHEN 'agent' THEN 'agent' WHEN 'check' THEN 'validator' ELSE 'delivery' END
        ORDER BY started_at DESC LIMIT 1)
    WHERE NEW.stage_id IS NOT NULL
      AND NEW.state NOT IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted');
END;

DROP TRIGGER human_questions_close_with_ticket;
CREATE TRIGGER human_questions_close_with_ticket AFTER UPDATE OF state ON tickets
WHEN NEW.state <> OLD.state
 AND NEW.state IN ('backlog', 'parked', 'pr_open', 'merged_into', 'merged', 'closed_not_merged')
BEGIN
    UPDATE human_questions
    SET answered_at = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER),
        answered_via = 'closed'
    WHERE ticket_id = NEW.id AND answered_at IS NULL;
END;
