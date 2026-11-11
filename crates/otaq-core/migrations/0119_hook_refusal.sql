-- A project's git hook refused one of Otaq's own git steps on this ticket (its merge, its start, its PR's messages):
-- JSON `{hook, operation, step, output, at}`, the hook's name and whole output (home paths shortened), for the card's
-- Needs you flag and the ticket's sheet. Cleared once the ticket leaves the stop the refusal put it in; History keeps
-- the refusal with its output. NULL on every other ticket. Never edit once committed.
ALTER TABLE tickets ADD COLUMN hook_refusal TEXT;
