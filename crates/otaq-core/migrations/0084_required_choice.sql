-- A skippable human check's choice that it happens reads "required" again (it read "you" for a while). The table
-- stores it as `usual`, so only History has the word: each `stage_choice_set` event's `choice` and `from` that read
-- "you" read "required". Never edit once committed.
UPDATE events
SET payload_json = json_set(payload_json, '$.choice', 'required')
WHERE kind = 'stage_choice_set' AND json_valid(payload_json) AND json_extract(payload_json, '$.choice') = 'you';

UPDATE events
SET payload_json = json_set(payload_json, '$.from', 'required')
WHERE kind = 'stage_choice_set' AND json_valid(payload_json) AND json_extract(payload_json, '$.from') = 'you';
