-- Comment attachments (tuq decisions 001, 002, 005, 006). Never edit once committed.
--
-- A ticket comment carries any number of attachments of any type, not one report (0055). One row per attachment:
-- `n` is its number in its comment (1, 2, 3…, in the order attached), `file` its path relative to the data folder as
-- written (`reports/<repo>/<KEY>/<comment id>/<n>/<name>`; a later key change still finds it), `name` what it is
-- called (its file's name, "Pasted text" for pasted text), `kind` image / text / other, `bytes` its size and `lines`
-- its line count, for text only. The rows go with their comment; the files are removed by Delete (d0060).
--
-- Every comment's report becomes its attachment 1, so old comments keep theirs: a copied file kept its own name in
-- its comment's folder (`…/<comment id>/<name>`), pasted text was `…/<comment id>.txt`. A report named like an image
-- is an image (no line count); every other report was read as text, and stays text. Then 0055's report columns go.
CREATE TABLE comment_attachments (
    comment_id TEXT   NOT NULL REFERENCES ticket_comments (id) ON DELETE CASCADE,
    n          BIGINT NOT NULL,
    file       TEXT   NOT NULL,
    name       TEXT   NOT NULL,
    kind       TEXT   NOT NULL CHECK (kind IN ('image', 'text', 'other')),
    bytes      BIGINT NOT NULL,
    lines      BIGINT,
    PRIMARY KEY (comment_id, n)
);

INSERT INTO comment_attachments (comment_id, n, file, name, kind, bytes, lines)
SELECT id, 1, report_file, name,
       CASE WHEN is_image THEN 'image' ELSE 'text' END,
       COALESCE(report_bytes, 0),
       CASE WHEN is_image THEN NULL ELSE COALESCE(report_lines, 0) END
FROM (
    SELECT id, report_file, report_bytes, report_lines, name,
           (lower(name) LIKE '%.png' OR lower(name) LIKE '%.jpg' OR lower(name) LIKE '%.jpeg'
            OR lower(name) LIKE '%.gif' OR lower(name) LIKE '%.webp' OR lower(name) LIKE '%.bmp'
            OR lower(name) LIKE '%.svg' OR lower(name) LIKE '%.avif' OR lower(name) LIKE '%.ico'
            OR lower(name) LIKE '%.tif' OR lower(name) LIKE '%.tiff' OR lower(name) LIKE '%.heic'
            OR lower(name) LIKE '%.heif') AS is_image
    FROM (
        SELECT id, report_file, report_bytes, report_lines,
               CASE WHEN instr(replace(report_file, '\', '/'), '/' || id || '/') > 0
                    THEN substr(replace(report_file, '\', '/'),
                                instr(replace(report_file, '\', '/'), '/' || id || '/') + length(id) + 2)
                    ELSE 'Pasted text'
               END AS name
        FROM ticket_comments
        WHERE report_file IS NOT NULL
    ) named
) kinds;

ALTER TABLE ticket_comments DROP COLUMN report_file;
ALTER TABLE ticket_comments DROP COLUMN report_bytes;
ALTER TABLE ticket_comments DROP COLUMN report_lines;
