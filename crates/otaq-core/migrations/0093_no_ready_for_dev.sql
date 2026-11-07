-- The Ready for Dev column goes: starting a ticket puts it into the first stage, and a ticket that can't begin yet
-- waits at the top of that stage's column. Never edit once committed.
--
-- A ticket that was in Ready for Dev (queued, waiting for a slot, held, parent closed, start interrupted; no stage)
-- already is in that state: it was started, nothing began for it, and it begins by itself when it can. Its row stays as
-- it is and now shows at the top of its flow's first stage. What goes is the column itself: a project whose board had
-- Ready for Dev folded from its menu forgets that fold.
UPDATE projects
SET settings_json = json_set(settings_json, '$.view.folded_columns',
                             (SELECT json_group_array(c.value)
                              FROM json_each(projects.settings_json, '$.view.folded_columns') c
                              WHERE c.value IS NOT 'ready_for_dev'))
WHERE json_valid(settings_json)
  AND json_type(settings_json, '$.view.folded_columns') = 'array'
  AND EXISTS (SELECT 1 FROM json_each(projects.settings_json, '$.view.folded_columns') c
              WHERE c.value = 'ready_for_dev');
