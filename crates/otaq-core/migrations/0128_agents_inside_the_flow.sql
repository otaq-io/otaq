-- There are no named agents: a stage of a flow says who does it (UX-65…UX-75). Never edit once committed.
--
-- Each stored flow's stages and delivery take their agent's model, effort and instructions, and "Noyan starts on"
-- takes the model and effort of the agent Noyan used: SQL can't write a flow document, so the rest runs in Rust right
-- after the migrations ([`Store::move_agents_into_flows`]), reading the agents from `agents_before_0128`, which it then
-- drops. What SQL can do is here:
--
-- - A ticket's choice of another agent for a stage is dropped (UX-72); its skips stay. `ticket_stage_choices` loses
--   `agent_id`.
-- - A hold, a session, a Noyan conversation and a stage run no longer keep an agent's id (`agent_id` goes). A session
--   keeps its tool, model, effort and the name History shows (`agent_name`), and says what it runs for (`runs_for`: a
--   stage, the delivery or Noyan; where a mark about its model goes).
-- - A model's amber mark (refused, no longer listed, swapped) is kept on what it is about (`model_marks`: a stage, a
--   delivery or Noyan's setting), not on an agent.

DELETE FROM ticket_stage_choices WHERE choice = 'agent';
CREATE TABLE ticket_stage_choices_new (
    ticket_id TEXT   NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    stage_id  TEXT   NOT NULL,
    choice    TEXT   NOT NULL CHECK (choice IN ('usual', 'skip')),
    set_by    TEXT   NOT NULL CHECK (set_by IN ('user', 'project_agent', 'agent')),
    by_name   TEXT,
    reason    TEXT,
    set_at    BIGINT NOT NULL,
    PRIMARY KEY (ticket_id, stage_id)
);
INSERT INTO ticket_stage_choices_new (ticket_id, stage_id, choice, set_by, by_name, reason, set_at)
SELECT ticket_id, stage_id, choice, set_by, by_name, reason, set_at FROM ticket_stage_choices;
DROP TABLE ticket_stage_choices;
ALTER TABLE ticket_stage_choices_new RENAME TO ticket_stage_choices;

CREATE TABLE agent_holds_new (
    id         TEXT   NOT NULL PRIMARY KEY,
    ticket_id  TEXT   REFERENCES tickets (id) ON DELETE CASCADE,
    project_id TEXT   REFERENCES projects (id) ON DELETE CASCADE,
    role       TEXT   NOT NULL CHECK (role IN ('developer', 'validator', 'project_agent')),
    reason     TEXT   NOT NULL CHECK (reason IN ('tool_limit', 'tool_unavailable', 'model_unavailable', 'missing_skill')),
    tool       TEXT   NOT NULL,
    model      TEXT,
    skill      TEXT,
    since      BIGINT NOT NULL,
    CHECK ((ticket_id IS NULL) <> (project_id IS NULL))
);
INSERT INTO agent_holds_new (id, ticket_id, project_id, role, reason, tool, model, skill, since)
SELECT id, ticket_id, project_id, role, reason, tool, model, skill, since FROM agent_holds;
DROP TABLE agent_holds;
ALTER TABLE agent_holds_new RENAME TO agent_holds;
CREATE UNIQUE INDEX agent_holds_project_role ON agent_holds (project_id, role) WHERE project_id IS NOT NULL;
CREATE UNIQUE INDEX agent_holds_ticket_role ON agent_holds (ticket_id, role) WHERE ticket_id IS NOT NULL;
CREATE INDEX agent_holds_tool ON agent_holds (tool, reason);

-- The triggers that copied a session's agent id onto its stage run, made again without it.
DROP TRIGGER stage_runs_take_round_sessions;
DROP TRIGGER stage_runs_take_session_models;
DROP TRIGGER stage_runs_take_sessions;
DROP TRIGGER stage_runs_follow_tickets;

ALTER TABLE sessions DROP COLUMN agent_id;
ALTER TABLE sessions ADD COLUMN runs_for TEXT;
ALTER TABLE stage_runs DROP COLUMN agent_id;
ALTER TABLE project_agents DROP COLUMN agent_id;

CREATE TRIGGER stage_runs_take_round_sessions AFTER UPDATE OF session_id ON validations
WHEN NEW.session_id IS NOT NULL
BEGIN
    UPDATE stage_runs
    SET session_id = NEW.session_id,
        agent = (SELECT agent_name FROM sessions WHERE id = NEW.session_id),
        model = (SELECT COALESCE(model_ran, model) FROM sessions WHERE id = NEW.session_id),
        effort = (SELECT effort FROM sessions WHERE id = NEW.session_id)
    WHERE validation_id = NEW.id;
END;

CREATE TRIGGER stage_runs_take_session_models AFTER UPDATE OF model_ran, agent_name ON sessions
BEGIN
    UPDATE stage_runs
    SET model = COALESCE(NEW.model_ran, NEW.model, model), agent = COALESCE(NEW.agent_name, agent)
    WHERE session_id = NEW.id;
END;

CREATE TRIGGER stage_runs_take_sessions AFTER INSERT ON sessions
WHEN NEW.ticket_id IS NOT NULL AND NEW.kind IN ('agent', 'validator', 'delivery')
BEGIN
    UPDATE stage_runs
    SET session_id = NEW.id, agent = NEW.agent_name,
        model = COALESCE(NEW.model_ran, NEW.model), effort = NEW.effort
    WHERE ticket_id = NEW.ticket_id AND ended_at IS NULL
      AND kind = CASE NEW.kind WHEN 'agent' THEN 'agent' WHEN 'validator' THEN 'check' ELSE 'delivery' END
      AND (NEW.kind != 'validator'
           OR (SELECT COUNT(*) FROM stage_runs
               WHERE ticket_id = NEW.ticket_id AND ended_at IS NULL AND kind = 'check') = 1);
END;

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
    INSERT INTO stage_runs (id, ticket_id, stage_id, kind, session_id, agent, model, effort, started_at)
    SELECT lower(hex(randomblob(16))), NEW.id, NEW.stage_id, k.kind, s.id, s.agent_name,
           COALESCE(s.model_ran, s.model), s.effort, CAST(unixepoch('subsec') * 1000 AS INTEGER)
    FROM (SELECT CASE
              WHEN NEW.state IN ('validating', 'validator_needs_you', 'validation_failed', 'validation_no_result')
                OR NEW.interrupted_from IN ('validating', 'validator_needs_you') THEN 'check'
              WHEN NEW.state = 'ready_for_review' THEN 'you'
              ELSE 'agent'
          END AS kind) k
    LEFT JOIN sessions s ON s.id = (
        SELECT id FROM sessions
        WHERE ticket_id = NEW.id AND ended_at IS NULL
          AND kind = CASE k.kind WHEN 'agent' THEN 'agent' WHEN 'check' THEN 'validator' ELSE 'delivery' END
        ORDER BY started_at DESC LIMIT 1)
    WHERE NEW.stage_id IS NOT NULL
      AND NEW.state NOT IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted',
                            'to_deliver', 'pr_open', 'merged_into');
END;

-- `scope`: `stage/<project key>/<stage id>`, `delivery/<project key>` or `noyan`.
CREATE TABLE model_marks (
    scope     TEXT   NOT NULL PRIMARY KEY,
    text      TEXT   NOT NULL,
    model     TEXT,
    ticket    TEXT,
    message   TEXT,
    marked_at BIGINT NOT NULL
);

-- Kept only for the Rust rest of this migration, which drops it.
DROP INDEX agents_name;
ALTER TABLE agents RENAME TO agents_before_0128;
