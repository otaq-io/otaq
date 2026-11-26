-- How a waiting ticket came to wait (tuq decisions 119, 120; tech spec slot-between-stages): `begin` (waiting to
-- begin, or before a stage after leaving something that held no slot), `kept` (it left an Agent stage or an agent
-- check, so it keeps its slot through the hand-over) or `after_check` (it left one of the user's checks). Written
-- with `queued_at` by every queue write ([`set_queued!`]); NULL outside the waiting statuses, and a NULL reads as
-- `begin`. Never edit once committed.
--
-- Every ticket that waits now waited the way it always did: `begin`.

ALTER TABLE tickets ADD COLUMN wait_kind TEXT CHECK (wait_kind IN ('begin', 'kept', 'after_check'));

UPDATE tickets SET wait_kind = 'begin'
 WHERE state IN ('queued', 'waiting_for_slot', 'held', 'parent_closed', 'start_interrupted');
