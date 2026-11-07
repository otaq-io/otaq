-- A stage runs only on a tool that has its skill. Never edit once committed.
--
-- A stage due to start on a tool that doesn't have the skill it names is held like a tool that can't run: its hold
-- (reason `missing_skill`) keeps that skill's name in `skill`, so the card says "Codex doesn't have the skill
-- `/team-validate`" and Check again asks the finder again. `skill` is null for every other hold. The table is made
-- again for the new reason (SQLite can't change a CHECK); every hold is kept as it was.
--
-- The stage no longer starts anyway with a warning, so a session no longer keeps the skill Otaq couldn't find.
CREATE TABLE agent_holds_new (
    id         TEXT   NOT NULL PRIMARY KEY,
    ticket_id  TEXT   REFERENCES tickets (id) ON DELETE CASCADE,
    project_id TEXT   REFERENCES projects (id) ON DELETE CASCADE,
    role       TEXT   NOT NULL CHECK (role IN ('developer', 'validator', 'project_agent')),
    reason     TEXT   NOT NULL CHECK (reason IN ('tool_limit', 'tool_unavailable', 'model_unavailable', 'missing_skill')),
    tool       TEXT   NOT NULL,
    model      TEXT,
    agent_id   TEXT   REFERENCES agents (id) ON DELETE SET NULL,
    skill      TEXT,
    since      BIGINT NOT NULL,
    CHECK ((ticket_id IS NULL) <> (project_id IS NULL))
);
INSERT INTO agent_holds_new (id, ticket_id, project_id, role, reason, tool, model, agent_id, since)
SELECT id, ticket_id, project_id, role, reason, tool, model, agent_id, since FROM agent_holds;
DROP TABLE agent_holds;
ALTER TABLE agent_holds_new RENAME TO agent_holds;
CREATE UNIQUE INDEX agent_holds_project_role ON agent_holds (project_id, role) WHERE project_id IS NOT NULL;
CREATE UNIQUE INDEX agent_holds_ticket_role ON agent_holds (ticket_id, role) WHERE ticket_id IS NOT NULL;
CREATE INDEX agent_holds_tool ON agent_holds (tool, reason);

ALTER TABLE sessions DROP COLUMN missing_skill;
