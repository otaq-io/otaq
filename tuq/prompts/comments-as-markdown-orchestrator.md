# Orchestrator · comments-as-markdown

- **Ticket:** none
- **Date:** 2026-11-15
- **Plan:** tuq/plans/comments-as-markdown.md
- **Tech spec:** none (planned straight from the decisions and the code)
- **Decisions:** 110, 111, 112

comments-as-markdown shows ticket comments as Markdown (110), gives the comment box GitHub's helpers while it stays a
text box (111) and removes the length limit from the user's comments while agents keep 2,000 characters (112). You
are the agent in charge of building it. You don't build the pieces: you start one agent per piece, check what it hands
back and merge it. Read the decisions and the plan first; each piece's details in the plan are its brief.

Build on the branch the maintainer names when they start this work: either the 0.3 branch or a new branch from `main`
after the release. Ask if it isn't named. Every piece's worktree needs `git submodule update --init` (`plugin/skills`
is a submodule).

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 A comment's Markdown is drawn by one shared renderer, 3 Prime nests a comment's headings under the comment, 4 One-line previews show a comment's words without Markdown marks, 5 The user's comments have no length limit; agents keep 2,000 (side by side)
- After 1: 2 Every place a comment shows uses the renderer, and long comments fold
- After 5: 6 The comment box never blocks the user on length
- After 1 and 6: 7 The comment box gets Write/Preview, a toolbar and shortcuts
- After 7: 8 Lists continue on Enter, and a URL pasted over selected text makes a link

<!-- tuq:order:end -->

## Running a piece

### In an Otaq build stage ($OTAQ_TICKET set, prime shows this Plan)

- Start a piece with `otaq helper start N --title "<the Plan's title>"`; when it has a note below, pass the note on
  stdin (`--stdin`).
- Check what it hands back with `otaq helper diff N` and the piece's tests in its worktree (`otaq helper list` names
  it).
- Merge with `otaq helper merge N`, one piece at a time, then start the pieces that waited for it.
- Follow the build stage's own rules for questions, hand-backs and re-plans; this prompt never replaces them.

### Anywhere else

- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, the decisions it builds on, and "commit on your branch; don't merge".
- Check its diff and the piece's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Every piece: run only the tests of what it changes (nextest filters for Rust, vitest for touched UI files), `cargo
fmt --check` and clippy `-D warnings` on changed crates, and the UI's lint and type check for touched UI files. Never
run the full `scripts/check.sh` in a piece: you run it once on the merged branch at the end. Unset `PORT` and every
`OTAQ_*` variable for tests. A test that flakes on file access times may be Defender reading new files: wait it out
and run it again. The Windows VM (VMPal "Windows") may be used without asking. Each piece's report lists what it
deliberately left unchanged and the tests that prove it.

### Piece 1

The safety rules (HTML as text, links limited to http/https, no images loaded) are the point of this piece: a test for
each, including `javascript:` links, `<img onerror=…>`, `<script>` and an HTML block. Before writing the renderer,
check how `@tiptap/markdown` handles HTML and single line breaks in the pinned version (3.32.0) and pick the narrowest
hook (a parser option, a token filter or escaping before parsing). Don't change the description's parsing to get it.
The description's round-trip tests are the guard.

### Piece 5

Only the author decides the limit: don't add a second limit table or a per-box setting. The CLI's agent-facing
refusal text stays word for word.

### Piece 7

Keyboard routing is shared app-wide: extend the existing focus-kept-keys mechanism rather than adding listeners in
the box. Prove ⌘K still opens the palette everywhere outside a comment box and a description.

## At the end

After the last merge, run `scripts/check.sh` once on the merged branch. Then open the app and post, in a scratch
project, a comment with every Markdown feature, a 50,000-character paste, and an agent's comment over 2,000
characters (refused). Check the thread, a diff comment, History's line and `otaq prime` by eye. Report what you saw;
don't push or open a PR unless the maintainer asks.
