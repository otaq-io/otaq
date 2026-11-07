-- A check has one agent, like an Agent stage. Never edit once committed.
--
-- A hold before a launch is a ticket's hold again, one per ticket and session role: the per check agent holds of a
-- round shared by several check agents go. Of a ticket's holds of one role the oldest stays; when it lifts, the
-- check starts again. Rounds stored before keep every agent's row and read as they were.
DELETE FROM agent_holds
WHERE ticket_id IS NOT NULL
  AND rowid NOT IN (SELECT MIN(rowid) FROM agent_holds WHERE ticket_id IS NOT NULL GROUP BY ticket_id, role);
DROP INDEX agent_holds_ticket_role;
ALTER TABLE agent_holds DROP COLUMN check_agent;
CREATE UNIQUE INDEX agent_holds_ticket_role ON agent_holds (ticket_id, role) WHERE ticket_id IS NOT NULL;
