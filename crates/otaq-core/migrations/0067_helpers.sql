-- no-transaction
-- M45 Helper sessions (T-0537). Decisions 0133, 0082, 0018, 0020, 0016, 0079. Never edit once committed.
--
-- A build stage's agent in charge may split its work among helpers (d0133): one row per helper of a ticket, named
-- after a piece of its Plan (piece 3 → "40.3"), with its own branch and worktree, its session, its status (working,
-- held for its tool's limit, sent back, being reviewed, merged), the latest note from the agent in charge, its own
-- port (a helper's dev server never takes the ticket's), and once merged the files its squash touched (the Files
-- tab's piece tags). `sessions.kind` gains `helper`.
--
-- No users yet (M33+): nothing is migrated. The sessions table is rebuilt the documented way, as 0066 did: foreign
-- keys off (outside a transaction, so dropping the old table cascades nothing), copy, swap, indexes and triggers back,
-- foreign keys on.
CREATE TABLE helpers (
    id          TEXT    NOT NULL PRIMARY KEY,
    ticket_id   TEXT    NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    piece       BIGINT  NOT NULL CHECK (piece >= 1),
    title       TEXT    NOT NULL,
    branch      TEXT    NOT NULL,
    worktree    TEXT    NOT NULL,
    session_id  TEXT,                                          -- its latest session (sessions.id)
    status      TEXT    NOT NULL CHECK (status IN ('working', 'held', 'sent_back', 'being_reviewed', 'merged')),
    note        TEXT,                                          -- the latest note from the agent in charge
    held_reason TEXT,                                          -- held: "Claude Code limit reached"
    port        BIGINT,                                        -- `$PORT` in its session
    started_at  BIGINT  NOT NULL,
    updated_at  BIGINT  NOT NULL,                              -- its last change: "last activity"
    merged_at   BIGINT,
    files_json  TEXT,                                          -- merged: the paths its squash touched
    UNIQUE (ticket_id, piece)
);
CREATE INDEX helpers_session ON helpers (session_id);

PRAGMA foreign_keys = OFF;
PRAGMA legacy_alter_table = ON;
BEGIN;

CREATE TABLE sessions_new (
    id                   TEXT   NOT NULL PRIMARY KEY,
    project_id           TEXT   NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    ticket_id            TEXT   REFERENCES tickets (id) ON DELETE CASCADE,  -- NULL for Project terminals and agents
    kind                 TEXT   NOT NULL CHECK (kind IN ('agent', 'answer', 'delivery', 'shell', 'project', 'run',
                                                         'validator', 'project_agent', 'helper')),
    pty_id               TEXT,
    tool_session_id      TEXT,
    state                TEXT   NOT NULL,
    started_at           BIGINT NOT NULL,
    ended_at             BIGINT,
    end_reason           TEXT,
    summary_dismissed_at BIGINT,
    project_agent_id     TEXT   REFERENCES project_agents (id) ON DELETE SET NULL,
    tool                 TEXT,
    model                TEXT,
    agent_id             TEXT,
    agent_name           TEXT,
    model_ran            TEXT,
    effort               TEXT,
    fallback_from        TEXT,
    fallback_reason      TEXT   CHECK (fallback_reason IN ('tool_limit', 'tool_unavailable', 'model_unavailable')),
    primed_at            BIGINT
);
INSERT INTO sessions_new (id, project_id, ticket_id, kind, pty_id, tool_session_id, state, started_at, ended_at,
                          end_reason, summary_dismissed_at, project_agent_id, tool, model, agent_id, agent_name,
                          model_ran, effort, fallback_from, fallback_reason, primed_at)
SELECT id, project_id, ticket_id, kind, pty_id, tool_session_id, state, started_at, ended_at, end_reason,
       summary_dismissed_at, project_agent_id, tool, model, agent_id, agent_name, model_ran, effort, fallback_from,
       fallback_reason, primed_at
FROM sessions;
DROP TABLE sessions;
ALTER TABLE sessions_new RENAME TO sessions;
CREATE INDEX sessions_ticket ON sessions (ticket_id, started_at);
CREATE INDEX sessions_project ON sessions (project_id);
CREATE INDEX sessions_tool_session ON sessions (tool_session_id);

CREATE TRIGGER stage_runs_take_session_models AFTER UPDATE OF model_ran, agent_name ON sessions
BEGIN
    UPDATE stage_runs
    SET model = COALESCE(NEW.model_ran, NEW.model, model), agent = COALESCE(NEW.agent_name, agent),
        agent_id = COALESCE(NEW.agent_id, agent_id)
    WHERE session_id = NEW.id;
END;

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

COMMIT;
PRAGMA legacy_alter_table = OFF;
PRAGMA foreign_keys = ON;
