-- no-transaction
-- M45 One card walks the flow (T-0533). Decisions 0133, 0108, 0059, 0079. Never edit once committed.
--
-- There is no epic type and no epic lead any more (d0133): any ticket may split its work among helpers. The ticket
-- loses its epic columns (`epic_id`, `child_number`, `next_child_number`, `kind`, the epic queue mode and the "added
-- by" column of a child that joined it); it keeps its External ID (d0059: every ticket has one) and its priority,
-- which is never `NULL` again (no child follows an epic); it gains the Plan (d0133: Markdown written by a planning
-- stage or Noyan, with who wrote it last and when). The lead's tables and columns go: key aliases (Move to epic), the
-- lead's notes, stuck children and suggestions, its wake deliveries, its questions' columns, the lead's marks and fix
-- children of recorded decisions, a Project agent row's epic, the `epic-lead` role and the lead's holds and stage
-- choices.
--
-- No users yet (M33+): nothing is migrated. Epics, their children and their leads are deleted first, with foreign keys
-- on so what hangs off them goes too; then the tables are rebuilt the documented way, as 0063 did: foreign keys off
-- (outside a transaction, so dropping an old table cascades nothing), copy, swap, indexes and triggers back, foreign
-- keys on.
DELETE FROM pending_deliveries WHERE kind IN ('lead_wake', 'lead_note');
DELETE FROM project_agents WHERE epic_id IS NOT NULL;
DELETE FROM ticket_comments WHERE author_kind = 'lead';
DELETE FROM manual_checks WHERE author_kind = 'lead';
DELETE FROM agent_holds WHERE role = 'lead';
DELETE FROM ticket_stage_choices WHERE set_by = 'epic_lead';
DELETE FROM decisions WHERE outcome = 'changed_fix_child';
DELETE FROM tickets WHERE epic_id IS NOT NULL;
DELETE FROM tickets WHERE kind = 'epic';
DELETE FROM events WHERE kind IN ('moved_to_epic', 'epic_queue_mode_set', 'joined_queue', 'lead_ended', 'lead_changed',
                                  'lead_suggestion', 'decision_marked', 'lead_question', 'lead_note', 'lead_stuck');
DROP TABLE lead_stuck;
DROP TABLE lead_notes;
DROP TABLE lead_suggestions;
DROP TABLE ticket_key_aliases;

PRAGMA foreign_keys = OFF;
PRAGMA legacy_alter_table = ON;
BEGIN;

CREATE TABLE tickets_new (
    id                       TEXT    NOT NULL PRIMARY KEY,
    project_id               TEXT    NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    number                   BIGINT  NOT NULL,
    title                    TEXT    NOT NULL,
    body                     TEXT    NOT NULL DEFAULT '',      -- "What and why" (Markdown)
    plan                     TEXT    NOT NULL DEFAULT '',      -- the Plan (Markdown, d0133)
    plan_by                  TEXT,                             -- the Plan's last writer (an actor)
    plan_at                  BIGINT,
    external_id              TEXT,                             -- PAY-123 (d0059)
    priority                 BIGINT  NOT NULL DEFAULT 3 CHECK (priority BETWEEN 1 AND 4),
    state                    TEXT    NOT NULL,
    outcome                  TEXT    CHECK (outcome IN ('merged', 'closed')),
    pinned_at                BIGINT,
    parked                   BOOLEAN NOT NULL DEFAULT FALSE,
    base_branch              TEXT,                             -- resolved base (d0051)
    branch_name              TEXT,
    branch_locked            BOOLEAN NOT NULL DEFAULT FALSE,
    base_sha                 TEXT,
    review_head_sha          TEXT,
    review_entered_at        BIGINT,
    worktree_path            TEXT,
    agent_port               BIGINT,
    run_port                 BIGINT,
    pr_url                   TEXT,
    pr_number                BIGINT,
    sent_back_round          BIGINT  NOT NULL DEFAULT 0,
    created_by               TEXT    NOT NULL,                 -- actor: the user or an agent (d0032 --actor)
    updated_by               TEXT,
    started_at               BIGINT,
    created_at               BIGINT  NOT NULL,
    updated_at               BIGINT  NOT NULL,
    start_problem_step       TEXT,
    start_problem_message    TEXT,
    signal_text              TEXT,
    signal_since             BIGINT,
    wakeup_at                BIGINT,
    last_signal_at           BIGINT,
    state_since              BIGINT,
    queued_at                BIGINT,
    merge_commit_sha         TEXT,
    sent_back_at             BIGINT,
    resume_skill             TEXT,
    conflicts_with           TEXT,
    conflict_files           TEXT,
    conflict_reason          TEXT,
    worktree_kept            BOOLEAN NOT NULL DEFAULT FALSE,
    reopened                 BOOLEAN NOT NULL DEFAULT FALSE,
    interrupted_from         TEXT,
    return_mode              TEXT    CHECK (return_mode IN ('resume', 'scratch')),
    resume_prompt            TEXT,
    created_in_session       TEXT,
    branch_pushed            BOOLEAN NOT NULL DEFAULT FALSE,
    pushed_differs           TEXT,
    pr_rework                BOOLEAN NOT NULL DEFAULT FALSE,
    ready_since              BIGINT,
    review_entry             TEXT,
    auto_merge_conflicts     BIGINT  NOT NULL DEFAULT 0,
    auto_merge_stop          TEXT,
    merged_automatically     BOOLEAN NOT NULL DEFAULT FALSE,
    build_state              TEXT    NOT NULL DEFAULT 'present' CHECK (build_state IN ('present', 'freed', 'fetching')),
    build_problem            TEXT,
    resource_name            TEXT,
    resource_state           TEXT    NOT NULL DEFAULT 'none'
        CHECK (resource_state IN ('none', 'created', 'failed', 'removed')),
    resource_problem         TEXT,
    schema_overlap           TEXT,
    stage_id                 TEXT,
    nothing_to_merge         TEXT,
    delivered_without_checks BOOLEAN NOT NULL DEFAULT FALSE,
    merge_check_due          BOOLEAN NOT NULL DEFAULT FALSE,
    remote_warning           TEXT,
    UNIQUE (project_id, number)
);
INSERT INTO tickets_new (id, project_id, number, title, body, external_id, priority, state, outcome, pinned_at, parked,
                         base_branch, branch_name, branch_locked, base_sha, review_head_sha, review_entered_at,
                         worktree_path, agent_port, run_port, pr_url, pr_number, sent_back_round, created_by,
                         updated_by, started_at, created_at, updated_at, start_problem_step, start_problem_message,
                         signal_text, signal_since, wakeup_at, last_signal_at, state_since, queued_at,
                         merge_commit_sha, sent_back_at, resume_skill, conflicts_with, conflict_files,
                         conflict_reason, worktree_kept, reopened, interrupted_from, return_mode, resume_prompt,
                         created_in_session, branch_pushed, pushed_differs, pr_rework, ready_since, review_entry,
                         auto_merge_conflicts, auto_merge_stop, merged_automatically, build_state, build_problem,
                         resource_name, resource_state, resource_problem, schema_overlap, stage_id,
                         nothing_to_merge, delivered_without_checks, merge_check_due, remote_warning)
SELECT id, project_id, number, title, body, external_id, COALESCE(priority, 3), state, outcome, pinned_at, parked,
       base_branch, branch_name, branch_locked, base_sha, review_head_sha, review_entered_at, worktree_path,
       agent_port, run_port, pr_url, pr_number, sent_back_round, created_by, updated_by, started_at, created_at,
       updated_at, start_problem_step, start_problem_message, signal_text, signal_since, wakeup_at, last_signal_at,
       state_since, queued_at, merge_commit_sha, sent_back_at, resume_skill, conflicts_with, conflict_files,
       conflict_reason, worktree_kept, reopened, interrupted_from, return_mode, resume_prompt, created_in_session,
       branch_pushed, pushed_differs, pr_rework, ready_since, review_entry, auto_merge_conflicts, auto_merge_stop,
       merged_automatically, build_state, build_problem, resource_name, resource_state, resource_problem,
       schema_overlap, stage_id, nothing_to_merge, delivered_without_checks, merge_check_due, remote_warning
FROM tickets;
DROP TABLE tickets;
ALTER TABLE tickets_new RENAME TO tickets;

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
              WHEN NEW.state IN ('to_deliver', 'pr_open') THEN 'delivery'
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
CREATE TRIGGER human_questions_close_with_ticket AFTER UPDATE OF state ON tickets
WHEN NEW.state <> OLD.state AND NEW.state IN ('backlog', 'parked', 'pr_open', 'merged', 'closed_not_merged')
BEGIN
    UPDATE human_questions
    SET answered_at = CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER),
        answered_via = 'closed'
    WHERE ticket_id = NEW.id AND answered_at IS NULL;
END;

CREATE TABLE decisions_new (
    id            TEXT   NOT NULL PRIMARY KEY,
    ticket_id     TEXT   NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    ordinal       BIGINT NOT NULL,
    question      TEXT   NOT NULL,
    built         TEXT   NOT NULL,
    other_options TEXT   NOT NULL DEFAULT '[]',
    why           TEXT   NOT NULL,
    recorded_by   TEXT   NOT NULL,
    recorded_at   BIGINT NOT NULL,
    outcome       TEXT   NOT NULL DEFAULT 'open' CHECK (outcome IN ('open', 'kept', 'changed_sent_back')),
    outcome_text  TEXT,
    outcome_at    BIGINT,
    UNIQUE (ticket_id, ordinal)
);
INSERT INTO decisions_new (id, ticket_id, ordinal, question, built, other_options, why, recorded_by, recorded_at,
                           outcome, outcome_text, outcome_at)
SELECT id, ticket_id, ordinal, question, built, other_options, why, recorded_by, recorded_at, outcome, outcome_text,
       outcome_at
FROM decisions;
DROP TABLE decisions;
ALTER TABLE decisions_new RENAME TO decisions;

CREATE TABLE human_questions_new (
    id                TEXT   NOT NULL PRIMARY KEY,
    ticket_id         TEXT   REFERENCES tickets (id) ON DELETE CASCADE,
    project_agent_id  TEXT   REFERENCES project_agents (id) ON DELETE CASCADE,
    question          TEXT   NOT NULL,
    options_json      TEXT   NOT NULL DEFAULT '[]',
    recommended       BIGINT,
    asked_at          BIGINT NOT NULL,
    answer            TEXT,
    answered_at       BIGINT,
    option_notes_json TEXT,
    answered_via      TEXT   CHECK (answered_via IN ('box', 'terminal', 'replaced', 'closed')),
    CHECK ((ticket_id IS NULL) <> (project_agent_id IS NULL))
);
INSERT INTO human_questions_new (id, ticket_id, project_agent_id, question, options_json, recommended, asked_at,
                                 answer, answered_at, option_notes_json, answered_via)
SELECT id, ticket_id, project_agent_id, question, options_json, recommended, asked_at, answer, answered_at,
       option_notes_json, answered_via
FROM human_questions;
DROP TABLE human_questions;
ALTER TABLE human_questions_new RENAME TO human_questions;
CREATE INDEX human_questions_ticket ON human_questions (ticket_id, asked_at);
CREATE INDEX human_questions_agent ON human_questions (project_agent_id, asked_at);

CREATE TABLE project_agents_new (
    id              TEXT    NOT NULL PRIMARY KEY,
    project_id      TEXT    NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name            TEXT,
    name_source     TEXT    CHECK (name_source IN ('agent', 'fallback', 'user')),
    tool            TEXT    NOT NULL,
    tool_session_id TEXT,
    model           TEXT,
    state           TEXT    NOT NULL CHECK (state IN ('running', 'ended', 'ended_by_restart', 'paused')),
    first_request   TEXT,
    limit_stopped   INTEGER NOT NULL DEFAULT 0,
    created_at      BIGINT  NOT NULL,
    ended_at        BIGINT,
    closed_at       BIGINT,
    agent_id        TEXT,
    agent_name      TEXT,
    model_ran       TEXT,
    effort          TEXT
);
INSERT INTO project_agents_new (id, project_id, name, name_source, tool, tool_session_id, model, state, first_request,
                                limit_stopped, created_at, ended_at, closed_at, agent_id, agent_name, model_ran,
                                effort)
SELECT id, project_id, name, name_source, tool, tool_session_id, model, state, first_request, limit_stopped,
       created_at, ended_at, closed_at, agent_id, agent_name, model_ran, effort
FROM project_agents;
DROP TABLE project_agents;
ALTER TABLE project_agents_new RENAME TO project_agents;
CREATE INDEX project_agents_project ON project_agents (project_id, created_at);

CREATE TABLE pending_deliveries_new (
    id           TEXT   NOT NULL PRIMARY KEY,
    session_id   TEXT   NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    kind         TEXT   NOT NULL CHECK (kind IN ('agent_message', 'ticket_change', 'continue_after_limit', 'skill',
                                                 'validation')),
    text         TEXT   NOT NULL,
    created_at   BIGINT NOT NULL,
    message_id   TEXT,
    message_part TEXT
);
INSERT INTO pending_deliveries_new (id, session_id, kind, text, created_at, message_id, message_part)
SELECT id, session_id, kind, text, created_at, message_id, message_part FROM pending_deliveries;
DROP TABLE pending_deliveries;
ALTER TABLE pending_deliveries_new RENAME TO pending_deliveries;
CREATE INDEX pending_deliveries_session ON pending_deliveries (session_id, created_at);

CREATE TABLE agents_new (
    id                TEXT   NOT NULL PRIMARY KEY,
    name              TEXT   NOT NULL,
    instructions      TEXT   NOT NULL DEFAULT '',
    tool              TEXT,
    model             TEXT,
    effort            TEXT,
    role              TEXT   CHECK (role IN ('noyan')),
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
SELECT id, name, instructions, tool, model, effort, CASE role WHEN 'noyan' THEN 'noyan' END, came_from,
       came_from_project, mark_text, mark_model, mark_ticket, mark_message, marked_at, created_at, updated_at
FROM agents;
DROP TABLE agents;
ALTER TABLE agents_new RENAME TO agents;
CREATE UNIQUE INDEX agents_name ON agents (lower(name));
CREATE UNIQUE INDEX agents_role ON agents (role) WHERE role IS NOT NULL;
DELETE FROM agent_skills WHERE shipped = 'epic-lead';

CREATE TABLE agent_holds_new (
    id             TEXT    NOT NULL PRIMARY KEY,
    ticket_id      TEXT    REFERENCES tickets (id) ON DELETE CASCADE,
    project_id     TEXT    REFERENCES projects (id) ON DELETE CASCADE,
    role           TEXT    NOT NULL CHECK (role IN ('developer', 'validator', 'project_agent')),
    reason         TEXT    NOT NULL CHECK (reason IN ('tool_limit', 'tool_unavailable', 'model_unavailable')),
    tool           TEXT    NOT NULL,
    model          TEXT,
    agent_id       TEXT    REFERENCES agents (id) ON DELETE SET NULL,
    fallback_tool  TEXT,
    fallback_held  BOOLEAN NOT NULL DEFAULT FALSE,
    fallback_asked BOOLEAN NOT NULL DEFAULT FALSE,
    since          BIGINT  NOT NULL,
    check_agent    TEXT    NOT NULL DEFAULT '',
    CHECK ((ticket_id IS NULL) <> (project_id IS NULL))
);
INSERT INTO agent_holds_new (id, ticket_id, project_id, role, reason, tool, model, agent_id, fallback_tool,
                             fallback_held, fallback_asked, since, check_agent)
SELECT id, ticket_id, project_id, role, reason, tool, model, agent_id, fallback_tool, fallback_held, fallback_asked,
       since, check_agent
FROM agent_holds;
DROP TABLE agent_holds;
ALTER TABLE agent_holds_new RENAME TO agent_holds;
CREATE UNIQUE INDEX agent_holds_project_role ON agent_holds (project_id, role) WHERE project_id IS NOT NULL;
CREATE UNIQUE INDEX agent_holds_ticket_role ON agent_holds (ticket_id, role, check_agent) WHERE ticket_id IS NOT NULL;
CREATE INDEX agent_holds_tool ON agent_holds (tool, reason);

CREATE TABLE ticket_stage_choices_new (
    ticket_id TEXT   NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    stage_id  TEXT   NOT NULL,
    choice    TEXT   NOT NULL CHECK (choice IN ('usual', 'agent', 'skip')),
    agent_id  TEXT   REFERENCES agents (id) ON DELETE CASCADE,
    set_by    TEXT   NOT NULL CHECK (set_by IN ('user', 'project_agent', 'agent')),
    by_name   TEXT,
    reason    TEXT,
    set_at    BIGINT NOT NULL,
    PRIMARY KEY (ticket_id, stage_id),
    CHECK ((choice = 'agent') = (agent_id IS NOT NULL))
);
INSERT INTO ticket_stage_choices_new (ticket_id, stage_id, choice, agent_id, set_by, by_name, reason, set_at)
SELECT ticket_id, stage_id, choice, agent_id, set_by, by_name, reason, set_at FROM ticket_stage_choices;
DROP TABLE ticket_stage_choices;
ALTER TABLE ticket_stage_choices_new RENAME TO ticket_stage_choices;

COMMIT;
PRAGMA legacy_alter_table = OFF;
PRAGMA foreign_keys = ON;
