-- M44 The user's actions with several check agents (T-0528). Decisions 0130 (row 17), 0108, 0086, 0079. Never edit
-- once committed.
--
-- A plan-limit hold (and every other hold before a launch) of an agent check is per held check agent (d0130 row 17):
-- `check_agent` is the check agent's name as the round's row names it; '' for every other hold (an Agent stage's, a
-- lead's, a Project agent's, a check round opened before its agents were named). One hold per ticket, role and check
-- agent.
ALTER TABLE agent_holds ADD COLUMN check_agent TEXT NOT NULL DEFAULT '';
DROP INDEX agent_holds_ticket_role;
CREATE UNIQUE INDEX agent_holds_ticket_role ON agent_holds (ticket_id, role, check_agent) WHERE ticket_id IS NOT NULL;
