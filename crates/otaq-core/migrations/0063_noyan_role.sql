-- no-transaction
-- M42 Noyan (T-0517). Decisions 0128, 0108, 0079; Q511. Never edit once committed.
--
-- The Project agent role is stored as `noyan` (d0128: the role key follows the role's name; was `project-agent`). An
-- agent that held `project-agent` holds `noyan`; `project-agent` is refused from now on. The shipped skill is `noyan`
-- too (was `project-agent`, whose folder is gone), so an agent's skill list names it that way. SQLite can't change
-- the role's CHECK in place, so `agents` is rebuilt the documented way, as 0057 did: foreign keys off (outside a
-- transaction, so dropping the old table cascades nothing into agent_skills and ticket_stage_choices, nor clears
-- agent_holds' agent), copy, swap, indexes back, foreign keys on.
PRAGMA foreign_keys = OFF;
BEGIN;
CREATE TABLE agents_new (
    id                TEXT   NOT NULL PRIMARY KEY,
    name              TEXT   NOT NULL,
    instructions      TEXT   NOT NULL DEFAULT '',
    tool              TEXT,
    model             TEXT,
    effort            TEXT,
    role              TEXT   CHECK (role IN ('noyan', 'epic-lead')),
    came_from         TEXT   NOT NULL CHECK (came_from IN ('agents_file', 'default_preset', 'you', 'flow')),
    came_from_project TEXT,
    mark_text         TEXT,
    mark_model        TEXT,
    mark_ticket       TEXT,
    mark_message      TEXT,
    marked_at         BIGINT,
    created_at        BIGINT NOT NULL,
    updated_at        BIGINT NOT NULL,
    CHECK (tool IS NOT NULL OR (model IS NULL AND effort IS NULL))
);
INSERT INTO agents_new (id, name, instructions, tool, model, effort, role, came_from, came_from_project, mark_text,
                        mark_model, mark_ticket, mark_message, marked_at, created_at, updated_at)
SELECT id, name, instructions, tool, model, effort,
       CASE role WHEN 'project-agent' THEN 'noyan' ELSE role END,
       came_from, came_from_project, mark_text, mark_model, mark_ticket, mark_message, marked_at, created_at,
       updated_at
FROM agents;
DROP TABLE agents;
ALTER TABLE agents_new RENAME TO agents;
CREATE UNIQUE INDEX agents_name ON agents (lower(name));
CREATE UNIQUE INDEX agents_role ON agents (role) WHERE role IS NOT NULL;
UPDATE agent_skills SET shipped = 'noyan' WHERE shipped = 'project-agent';
COMMIT;
PRAGMA foreign_keys = ON;
