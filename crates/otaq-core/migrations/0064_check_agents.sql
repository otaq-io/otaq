-- M44 Several check agents in one Check stage (T-0527). Decisions 0130, 0108, 0063, 0079. Never edit once committed.
--
-- A round of an agent check runs every check agent the stage lists, each in its own read-only session (d0130): one
-- `validations` row per agent per round. `agent` is the check agent's name as the stage lists it (the ticket's
-- chosen option for the first one); `round_id` ties the rows of one round together (the id of its first row; NULL
-- reads as the row's own id, a round of one). Rows ended earlier (no users yet) stay rounds of one.
ALTER TABLE validations ADD COLUMN agent TEXT;
ALTER TABLE validations ADD COLUMN round_id TEXT;
CREATE INDEX validations_round ON validations (round_id);

-- A round's rows each take a run of their own (STG-05), with the agent before its session starts: the first takes the
-- run the ticket opened by entering the check; the others are new runs.
DROP TRIGGER stage_runs_take_rounds;
CREATE TRIGGER stage_runs_take_rounds AFTER INSERT ON validations
BEGIN
    UPDATE stage_runs SET round = NEW.round, validation_id = NEW.id, started_at = NEW.started_at,
                          agent = COALESCE(NEW.agent, agent)
    WHERE id = (SELECT id FROM stage_runs
                WHERE ticket_id = NEW.ticket_id AND ended_at IS NULL AND kind = 'check' AND validation_id IS NULL
                ORDER BY started_at DESC LIMIT 1);
    INSERT INTO stage_runs (id, ticket_id, stage_id, kind, agent, round, validation_id, started_at)
    SELECT lower(hex(randomblob(16))), NEW.ticket_id, t.stage_id, 'check', NEW.agent, NEW.round, NEW.id,
           NEW.started_at
    FROM tickets t
    WHERE t.id = NEW.ticket_id AND t.stage_id IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM stage_runs WHERE validation_id = NEW.id);
END;

-- A new session takes the ticket's open run of its kind, as before, except a checker while several check runs are
-- open: its own row names it (`stage_runs_take_round_sessions`), so it never takes another agent's run.
DROP TRIGGER stage_runs_take_sessions;
CREATE TRIGGER stage_runs_take_sessions AFTER INSERT ON sessions
WHEN NEW.ticket_id IS NOT NULL AND NEW.kind IN ('agent', 'validator', 'delivery')
BEGIN
    UPDATE stage_runs
    SET session_id = NEW.id, agent_id = NEW.agent_id, agent = NEW.agent_name,
        model = COALESCE(NEW.model_ran, NEW.model), effort = NEW.effort
    WHERE ticket_id = NEW.ticket_id AND ended_at IS NULL
      AND kind = CASE NEW.kind WHEN 'agent' THEN 'agent' WHEN 'validator' THEN 'check' ELSE 'delivery' END
      AND (NEW.kind != 'validator'
           OR (SELECT COUNT(*) FROM stage_runs
               WHERE ticket_id = NEW.ticket_id AND ended_at IS NULL AND kind = 'check') = 1);
END;
