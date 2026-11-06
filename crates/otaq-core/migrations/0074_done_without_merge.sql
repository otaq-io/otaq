-- no-transaction
-- "Done, nothing to merge" (approve options). Never edit once committed.
--
-- The Approve menu can finish a ticket without a merge: it ends Merged (tickets waiting on it start) with the outcome
-- `done_without_merge`, no merge commit, and a reviews row with the action `done_without_merge`. Both columns are
-- CHECKed, and SQLite can't change a CHECK in place, so `tickets` and `reviews` are rebuilt the documented way, as
-- 0066 rebuilt `tickets`: foreign keys off (outside a transaction, so dropping the old tables cascades nothing), copy,
-- swap, indexes and triggers back, foreign keys on. Every row and id stays.
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
    outcome                  TEXT    CHECK (outcome IN ('merged', 'closed', 'done_without_merge')),
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
INSERT INTO tickets_new (id, project_id, number, title, body, plan, plan_by, plan_at, external_id, priority, state, outcome, pinned_at, parked,
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
SELECT id, project_id, number, title, body, plan, plan_by, plan_at, external_id, priority, state, outcome, pinned_at, parked,
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

CREATE TABLE reviews_new (
    id        TEXT   NOT NULL PRIMARY KEY,
    ticket_id TEXT   NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    round     BIGINT NOT NULL,
    action    TEXT   NOT NULL
              CHECK (action IN ('send_back', 'approve_local', 'open_pr', 'push', 'done_without_merge')),
    at        BIGINT NOT NULL
);
INSERT INTO reviews_new (id, ticket_id, round, action, at)
SELECT id, ticket_id, round, action, at
FROM reviews;
DROP TABLE reviews;
ALTER TABLE reviews_new RENAME TO reviews;
CREATE INDEX reviews_ticket ON reviews (ticket_id, round);

COMMIT;
PRAGMA legacy_alter_table = OFF;
PRAGMA foreign_keys = ON;
