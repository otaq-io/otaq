# Orchestrator · comment-attachments

- **Ticket:** none
- **Date:** 2026-11-07
- **Plan:** tuq/plans/comment-attachments.md
- **Tech spec:** none (no tech spec was written; tuq/decisions/001–008 and the code named in the plan)
- **Decisions:** 001, 002, 003, 004, 005, 006, 007, 008

Comment attachments: several files of any type per ticket comment, added by paste, drop or Attach, with image
thumbnails, for users and agents alike.

You are the agent in charge of building comment-attachments. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 A comment carries several attachments of any type
- After 1: 2 The user adds files in the comment box by paste, drop or Attach, 3 Posted images show as thumbnails that open full size (side by side)
- After 2: 4 A comment can be sent with only attachments
- After 4: 5 Editing a comment can add and remove its attachments

<!-- tuq:order:end -->

## Running a piece

### In an Otaq build stage ($OTAQ_TICKET set, prime shows this Plan)

- Start a piece with `otaq helper start N --title "<the Plan's title>"`; when it has a note below, pass the note on
  stdin (`--stdin`).
- Check what it hands back with `otaq helper diff N` and the project's tests in its worktree (`otaq helper list`
  names it).
- Merge with `otaq helper merge N`, one piece at a time, then start the pieces that waited for it.
- Follow the build stage's own rules for questions, hand-backs and re-plans; this prompt never replaces them.

### Anywhere else

- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge".
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

### Piece 1

- It is the widest piece and everything else builds on its data shape: check its diff carefully before merging, and
  pass its final names (the attachment kinds, the request fields, how `otaq report` picks one attachment) into the
  briefs of pieces 2–5 so they don't guess.
- The migration must carry every existing report over; ask for a test that runs it on a database holding a comment
  with a report.
- Run `cargo test` for otaq-core, otaq-view, otaq-cli and otaq-desktop, and the UI's tests, before merging: the type
  change ripples into the generated TypeScript.

### Piece 2

- Clipboard images have no file path: watch that the bytes are not base64-encoded into a request so large it stalls
  the page for a 20 MB file; refuse over the limit in the page before sending.

### Piece 3

- Watch the security of how images reach the page: only files inside Otaq's data folder (the reports folder) may be
  served, never an arbitrary path from the page.

### Pieces 2 and 3

- They run side by side and both touch the comment chips in CommentThread.tsx / editor.css: merge one, then have the
  other rebase before its merge.

## Bringing it together

After the last merge: run the project's full tests, try each piece's Check: line on the merged branch, and
in the running app (the `run` skill) go through the whole feature once: paste a screenshot and drop a .txt and a
.zip into the thread's box, send with no text, see the thumbnail and two chips, open the image full size, edit the
comment to remove the .zip and add another image, and check History shows both changes. Then have an agent post a
screenshot with `otaq comment --report-file` and check it shows as a thumbnail and in `otaq prime`.
