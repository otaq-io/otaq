-- Editing a comment can remove its attachments (tuq 008). Never edit once committed.
--
-- `removed_at`: when an edit took the attachment off its comment. The row and its file stay: History's edit row names
-- it and still opens it, as it keeps a comment's old text; the thread, `prime` and `otaq comments` list only the rows
-- without it. Ticket delete removes the file with the others (d0060). An attachment added by a later edit takes the
-- next number after every one the comment ever had, removed ones included, so numbers and files are never reused.
ALTER TABLE comment_attachments ADD COLUMN removed_at BIGINT;
