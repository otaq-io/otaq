-- Otaq has no fallback agents. Never edit once committed.
--
-- When a tool can't run (its limit, logged out, not installed, hidden, needing an update) or its tool no longer lists
-- an agent's model, that agent's work waits for it: nothing moves to another tool. The stored fallback per tool goes,
-- with no notice. A hold the user had sent to a fallback that hasn't started there yet waits for its own tool again,
-- like every other hold. A session that already runs on a fallback keeps its tool and model (the columns `tool` and
-- `model`) and finishes there; only the note of why it runs there goes. History keeps its old "Started on …" events.
DELETE FROM settings WHERE key IN ('agents.fallback', 'agents.auto_fallback');
ALTER TABLE agent_holds DROP COLUMN fallback_tool;
ALTER TABLE agent_holds DROP COLUMN fallback_held;
ALTER TABLE agent_holds DROP COLUMN fallback_asked;
ALTER TABLE sessions DROP COLUMN fallback_from;
ALTER TABLE sessions DROP COLUMN fallback_reason;
