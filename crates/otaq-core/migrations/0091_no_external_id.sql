-- Tickets have no External ID any more: the column goes, and History forgets every change to it. An edit that changed
-- only the External ID leaves History (and the "Updated by agent" marker); an edit that changed more keeps its other
-- fields. A fix an agent made to a ticket it filed loses "external_id" from its fields the same way. Never edit once
-- committed.
DELETE FROM events
WHERE kind = 'updated' AND json_valid(payload_json)
  AND EXISTS (SELECT 1 FROM json_each(events.payload_json, '$.changes') c
              WHERE json_extract(c.value, '$.field') = 'external_id')
  AND NOT EXISTS (SELECT 1 FROM json_each(events.payload_json, '$.changes') c
                  WHERE json_extract(c.value, '$.field') IS NOT 'external_id');

UPDATE events
SET payload_json = json_set(payload_json, '$.changes',
                            (SELECT json_group_array(json(c.value)) FROM json_each(events.payload_json, '$.changes') c
                             WHERE json_extract(c.value, '$.field') IS NOT 'external_id'))
WHERE kind = 'updated' AND json_valid(payload_json)
  AND EXISTS (SELECT 1 FROM json_each(events.payload_json, '$.changes') c
              WHERE json_extract(c.value, '$.field') = 'external_id');

DELETE FROM events
WHERE kind = 'fixed_created_ticket' AND json_valid(payload_json)
  AND json_extract(payload_json, '$.change') = 'edited'
  AND EXISTS (SELECT 1 FROM json_each(events.payload_json, '$.fields') f WHERE f.value = 'external_id')
  AND NOT EXISTS (SELECT 1 FROM json_each(events.payload_json, '$.fields') f WHERE f.value IS NOT 'external_id');

UPDATE events
SET payload_json = json_set(payload_json, '$.fields',
                            (SELECT json_group_array(f.value) FROM json_each(events.payload_json, '$.fields') f
                             WHERE f.value IS NOT 'external_id'))
WHERE kind = 'fixed_created_ticket' AND json_valid(payload_json)
  AND EXISTS (SELECT 1 FROM json_each(events.payload_json, '$.fields') f WHERE f.value = 'external_id');

ALTER TABLE tickets DROP COLUMN external_id;
