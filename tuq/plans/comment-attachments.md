# Plan · comment-attachments

- **Ticket:** none
- **Date:** 2026-11-07
- **Decisions:** 001, 002, 003, 004, 005, 006, 007, 008
- **Tech spec:** none (no tech spec was written; tuq/decisions/001–008 and the code named below)
- **Orchestrator prompt:** tuq/prompts/comment-attachments-orchestrator.md

<!-- tuq:plan:start -->

Comment attachments are built in five pieces, one behaviour each, following decisions 001–008. Piece 1 comes first:
it replaces the one-report-per-comment model with several attachments of any type and shows them through the command
line, so agents get them (006) and every later piece builds on its data. Pieces 2 (the user adds files in the comment
box) and 3 (posted images as thumbnails) both need only piece 1 and run side by side. Piece 4 (a comment of only
attachments) changes the box's send rule, so it waits for piece 2; piece 5 (editing a comment's attachments) reuses
the box's adding and the new send rule, so it waits for 4. The agent in charge follows
tuq/prompts/comment-attachments-orchestrator.md.

## Pieces

1. A comment carries several attachments of any type — waits for nothing
   Decisions 001, 002, 005, 006. Today a comment has at most one "report": the `report_file`, `report_bytes`,
   `report_lines` columns of `ticket_comments` (crates/otaq-core/migrations/0055_ticket_comments.sql), copied by
   `Board::prepare_report` / `copy_report` in crates/otaq-core/src/board/reports.rs and posted by `post_comment_in` in
   crates/otaq-core/src/board/ticket_comments.rs. Replace that with a list of attachments per comment, in a new
   migration (the next number after 0073; never edit a committed one) that moves every existing report into it, so
   old comments keep theirs. Each attachment keeps its name ("Pasted text" for pasted text), size, line count only
   for text, and a kind (image, text, other) derived from its type. Any file type is accepted (002); each file is at
   most 20 MB (`REPORT_LIMIT_BYTES`) and a comment may have any number (005); a refusal names the file
   ("perf.mov is 212 MB; the limit is 20 MB…") and posts nothing, leaving no copied file behind. Ticket delete removes
   every attachment file (`report_files_of`, crates/otaq-core/src/board/delete.rs). Update every reader of
   `TicketComment::report` / `report_label` / `report_line`: crates/otaq-core/src/prime.rs (the comment's lines and
   "Reports from you": list every attachment, with "open this path to view it" for images and others, never their
   bytes), hand_over.rs, tickets.rs, review/drag.rs, recovery/mod.rs, service/board_requests.rs, protocol.rs
   (`report_file` becomes a list in `StageEndParams`, `VerdictParams` and the comment requests), and the view models in
   crates/otaq-view (ticket_comments.rs: one chip per attachment under a comment; overview.rs `attachments`: one row
   per attachment; history.rs: "with 3 attachments (2.1 MB)" and a deleted comment still opens each). CLI
   (crates/otaq-cli/src/cli.rs, comment.rs): `--report-file` may be repeated on `comment`, `stage next|done` and
   `check pass|fail`; `otaq report KEY N` gains a way to pick one attachment of comment N (and `--path` gives its path;
   it never prints an image's bytes to stdout); `otaq comments --json` lists them. The app's comment box still sends
   at most one attachment here (piece 2 adds more); the New ticket form keeps its one report (RPT-03). Text
   attachments still open in the report view (crates/otaq-desktop/src/api/report.rs); images and other files don't
   open there (piece 3 opens images). One behaviour across the store, the service, the CLI and the view models, on
   purpose. Run the ts-rs export so crates/otaq-desktop/ui/src/generated is up to date.
   Check: core tests post a comment with three files (a PNG, a .txt, a .zip) through `otaq comment --report-file`
   repeated; `otaq comments --json` lists the three with kind and size; `otaq report` picks each; `prime` lists all
   three; a 21 MB file is refused by name and posts nothing; an old comment's report shows after the migration; ticket
   delete removes the files.
2. The user adds files in the comment box by paste, drop or Attach — waits for 1
   Decisions 001, 002, 003, 005. In crates/otaq-desktop/ui/src/screens/editor/CommentBox.tsx and commentRules.ts the
   box holds one `file` and one `pasted`; make it hold a list of attachments, each its own chip with ✕, sent together
   by `ticket_comments.post` (crates/otaq-desktop/src/api/ticket_comments.rs, which today takes `file` / `pasted`).
   Three ways in, in every comment box (the thread's and the Send back, Fail and drag-back dialogs:
   screens/review/Dialogs.tsx, CheckDialogs.tsx, screens/board/DragBackDialog.tsx): **paste** — an image on the
   clipboard (a screenshot) becomes an image attachment, files copied in Finder become attachments under their own
   names (today `onPaste` calls `file.text()` on any file, turning a screenshot into "Pasted text": that is the bug
   this fixes), text over the limit still becomes the "Pasted text" chip; **drag and drop** onto the box (see how
   crates/otaq-desktop/ui/src/terminal/drops.ts and Overview take dropped paths); an **Attach** button by the box
   opening the system picker with several files allowed. Clipboard image bytes have no path, so the request needs a
   way to carry bytes with a name and type. An image chip shows a small thumbnail of the image in the box (002); text
   and other files show name and size. A file over 20 MB is refused when it is added, with a message naming it, and
   the rest of the box stays (005). Overview's Add and drop (screens/overview/Overview.tsx, `attachReport`) add to
   the list instead of replacing the one file. One behaviour across the desktop API and the box, on purpose.
   Check: UI tests (comments.test.ts) for the list: paste of an image file adds an image chip with a thumbnail, two
   dropped files add two chips, ✕ removes one, a 21 MB file is refused by name with the text kept; in the running app,
   ⌘V of a screenshot then Comment posts it and the thread lists it.
3. Posted images show as thumbnails that open full size — waits for 1
   Decision 004 (and 002's preview rule). Under a posted comment in the thread
   (crates/otaq-desktop/ui/src/screens/editor/CommentThread.tsx, view model crates/otaq-view/src/view_models/
   ticket_comments.rs), its images show as a row of small thumbnails after its text; clicking one opens the image full
   size inside the app, over the sheet, with a way to close it. Text attachments keep their chip that opens the report
   view; other files are a plain chip with name and size. The same thumbnails show for agents' images (006) and in
   Overview's Attachments column (screens/overview/Overview.tsx) and in History's view of a posted or deleted comment
   (screens/activity/History.tsx). The page has to be able to load an attachment's image from the data folder: today
   the CSP in crates/otaq-desktop/tauri.conf.json is `img-src 'self' data:`, so either allow a narrow source for that
   folder or serve the image's bytes through a request; never expose the whole disk. One behaviour across the desktop
   API, the view model and the page, on purpose.
   Check: a view-model test gives a comment with two images and a .txt two thumbnails and one chip; in the running app
   a comment posted with `otaq comment --report-file shot.png` shows its thumbnail, a click opens it full size, and it
   shows in the Attachments column.
4. A comment can be sent with only attachments — waits for 2
   Decision 007. Today a comment with only a "Pasted text" chip can't be sent: `boxState` in
   crates/otaq-desktop/ui/src/screens/editor/commentRules.ts turns the button off and shows `needs_line`
   (`CommentBoxView`, crates/otaq-view/src/view_models/ticket_comments.rs, "With only a pasted report, the comment
   still needs its own line"), and the service refuses an empty body (`check_body`, the empty-comment refusal in
   crates/otaq-core/src/board/ticket_comments.rs). Change the rule everywhere to: text, or at least one attachment,
   or both; an empty comment (no text, no attachments) is still refused. Remove the `needs_line` hint. It holds in
   every comment box and for agents: `otaq comment` with `--report-file` and empty stdin posts, and `stage next|done`
   and `check pass|fail` accept an empty summary when a file is attached. Every place that shows a comment's text
   (the thread, `prime`'s comment heading and lines, History, Overview's "With comment N" line, the hand-over
   messages) reads well with no text, for example by naming the attachments instead. One behaviour across the service,
   the CLI, the view models and the box, on purpose.
   Check: core tests post a comment with one attachment and no text, and refuse one with neither; a UI test shows
   Comment enabled with only an image chip; `prime` and History render that comment without an empty line.
5. Editing a comment can add and remove its attachments — waits for 4
   Decision 008. Today `Board::edit_ticket_comment` (crates/otaq-core/src/board/ticket_comments.rs) changes only the
   text and logs `CommentEdited` with before / after. Editing a comment in the thread now shows its attachments as
   chips with ✕ and takes new ones by paste, drop and Attach, exactly as piece 2's box does; saving applies the text
   and the attachment changes together. A removed attachment leaves the comment and the thread, but its file stays
   and History's edit row shows which attachments were removed and added, and still opens a removed one
   (crates/otaq-view/src/view_models/history.rs). Saving follows 007: the comment must keep text or an attachment.
   Agents get the same through the CLI: `otaq comment edit` takes `--report-file` to add and a way to remove one by
   its number. One behaviour across the service, the CLI, History and the thread, on purpose.
   Check: core tests edit a comment to remove one of two attachments and add a third; the thread lists the new set,
   History's edit row names the removed and added ones and still opens the removed file, and an edit that would leave
   no text and no attachment is refused.

<!-- tuq:plan:end -->
