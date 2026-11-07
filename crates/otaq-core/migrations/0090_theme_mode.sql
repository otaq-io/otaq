-- The theme setting becomes a mode (Match system, Light or Dark) with one light and one dark theme. Never edit once
-- committed.
--
-- The one theme picked before becomes the pick of its kind, and the mode stays on that kind, so the window looks as
-- it did: a light theme becomes the light pick with the mode Light (Otaq Dark the dark pick), a dark theme the dark
-- pick with the mode Dark (Otaq Light the light pick). A setting still waiting for the OS (a dark and a light theme,
-- stored before the app's first start) becomes Match system with those two. With no `theme` row nothing is written:
-- the default is Match system.
UPDATE settings
SET value_json = CASE
    WHEN json_type(value_json) = 'text'
        AND json_extract(value_json, '$') IN
            ('otaq-light', 'catppuccin-latte', 'daylight', 'solarized-light', 'rose-pine-dawn')
        THEN json_object('mode', 'light', 'light', json_extract(value_json, '$'), 'dark', 'otaq-dark')
    WHEN json_type(value_json) = 'text'
        THEN json_object('mode', 'dark', 'light', 'otaq-light', 'dark', json_extract(value_json, '$'))
    WHEN json_type(value_json) = 'object'
        THEN json_object(
            'mode', 'system',
            'light', json_extract(value_json, '$.light'),
            'dark', json_extract(value_json, '$.dark'))
    ELSE value_json
END
WHERE key = 'theme' AND json_valid(value_json);
