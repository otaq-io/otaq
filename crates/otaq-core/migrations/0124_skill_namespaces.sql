-- Otaq's skills are named in their namespace: `otaq:validate`, `tuq:product-spec` (tuq decisions 063, 075). Never
-- edit once committed.
--
-- A flow version can now be applied by Otaq itself (`applied_by = 'skill_rename'`): the one-time rename of Otaq's
-- skills in every project's flow, which History shows as Otaq's rename, not the user's change. SQLite can't change a
-- CHECK, so the table is made again (nothing references it); every version is kept as it was stored.
--
-- Right after this migration, Store::open writes that version for every project whose current flow names one of
-- Otaq's skills by its bare name (`validate` becomes `otaq:validate`, `product-spec` becomes `tuq:product-spec`), and
-- rewrites the open proposals and waiting changes the same way. From then on a bare name is always the user's skill.
CREATE TABLE project_flows_new (
    project_id TEXT    NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    version    INTEGER NOT NULL,
    yaml       TEXT    NOT NULL,
    applied_by TEXT    NOT NULL CHECK (applied_by IN ('shipped', 'user', 'project_agent', 'skill_rename')),
    agent_name TEXT,
    preset     TEXT,
    applied_at BIGINT  NOT NULL,
    current    BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (project_id, version)
);
INSERT INTO project_flows_new (project_id, version, yaml, applied_by, agent_name, preset, applied_at, current)
SELECT project_id, version, yaml, applied_by, agent_name, preset, applied_at, current FROM project_flows;
DROP TABLE project_flows;
ALTER TABLE project_flows_new RENAME TO project_flows;
CREATE UNIQUE INDEX project_flows_current ON project_flows (project_id) WHERE current;

-- A ticket sent back or in a merge conflict resumes with Otaq's skill, now named in its namespace.
UPDATE tickets SET resume_skill = 'otaq:' || resume_skill
WHERE resume_skill IN ('address-review', 'restack', 'work-ticket', 'answer-question');
