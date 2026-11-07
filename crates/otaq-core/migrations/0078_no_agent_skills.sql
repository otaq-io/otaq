-- Agents have no skills (T6). Never edit once committed.
--
-- An agent is its instructions, its tool and model, and its role: a stage's `skill:` is the only place a skill is
-- picked, and Otaq never imports or copies a user's skill. The agents' skill lists go, imported ones included, with no
-- carry-over to a stage's `skill:`; a stage that ran an imported skill now runs Otaq's own. No other table or column
-- refers to `agent_skills`. Otaq's copies of imported skills (`$OTAQ_HOME/skills/`) are deleted at the service's start.
DROP INDEX IF EXISTS agent_skills_imported;
DROP TABLE agent_skills;
