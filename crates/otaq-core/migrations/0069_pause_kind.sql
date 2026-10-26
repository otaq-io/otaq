-- M49 How a project was paused (T-0564; PAU-01, PAU-06). Decisions 0112 (refined 2026-10-27), 0139. Never edit once
-- committed.
--
-- `paused_by` says how a paused project got paused, set with `paused_since` and cleared with it: `own` by its own
-- Pause (scope This project), `all` by an All projects pause (or Stop agents and quit). Resume all projects lifts only
-- the `all` ones, so a project paused on its own stays paused with its own banner and Resume (d0112 2026-10-27).
ALTER TABLE projects ADD COLUMN paused_by TEXT CHECK (paused_by IN ('own', 'all'));
