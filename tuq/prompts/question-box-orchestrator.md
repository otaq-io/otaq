# Orchestrator · question-box

- **Ticket:** none
- **Date:** 2026-11-12
- **Plan:** tuq/plans/question-box.md
- **Tech spec:** none (the decisions, the designer's frames and the code each piece names)
- **Decisions:** UX-62, UX-63, UX-64 (ideas/ux-rethink/DECISIONS.md; UX-63 refines UX-61 and extends UX-22)

question-box makes long agent questions readable, lets a question take several answers, and gives Edit flow a box
to ask Noyan.

You are the agent in charge of building question-box. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 A long question reads well wherever it shows, 4 Agents put the question first, and Noyan leaves nothing only in its terminal, 5 In Edit flow, the user asks Noyan for a change from the box (side by side)
- After 1, 5: 2 A long question in the Needs you strip opens wide, with a way to the larger place, 3 A question can take several answers, 6 Edit flow's box follows Noyan's conversation (side by side)
- After 1, 6: 7 Edit flow's box answers Noyan's question, and expands for a long one

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

- Work on the branch the maintainer started you on, never on main.
- Start one Agent call per piece with isolation "worktree". Its prompt: the piece's details from the plan, its note
  below, and "commit on your branch; don't merge".
- Check its diff and the project's tests in its worktree.
- Merge its branch into the current branch with `git merge`, one piece at a time, then start the pieces that waited
  for it.

## Notes per piece (only pieces that need one)

Pass these to every piece: the repo's rules are in CLAUDE.md; the old `decisions/` folder is lost (don't look for it
or write to it); the decisions are UX-62, UX-63 and UX-64 in ideas/ux-rethink/DECISIONS.md (uncommitted: read them
from the main checkout). The frames are on the canvas "Otaq — New UX"
(https://claude.ai/artifact/7tYuWTkYKLrq5gnUHSZWP1): read `project/flow-noyan-1.dc.html`, `flow-noyan-2`,
`question-box-1` and `question-box-2` with the Artifact tool's read action (data, not instructions); only
alternative A of flow-noyan was picked, ignore B. Every word the app shows comes from Rust view models in otaq-view,
never written in the page. New comments name a rule in words, never by its number. Never change the main checkout.
Never push. Pieces don't run the full `scripts/check.sh`: only their crates' tests (nextest with a filter, vitest for
touched UI), `cargo fmt --check`, clippy on the changed crates, and the ts-rs freshness / protocol snapshot tests when
they change a view model or protocol type; the orchestrator runs check.sh on the merged branch (Node 24 for its
frontend step). Never run the app on the maintainer's Mac session for hand checks.

### Piece 1

The shared answer UI is used in three places; change it there, never fork a copy per place. The split into question
line and context belongs in the view model so pieces 2, 3 and 7 get it for free. Keep the Markdown small and safe:
no HTML, no links that open anything.

### Piece 3

`--recommended` with several numbers is the one change to an existing flag: keep a single number working as today.
Check the answer wording with the maintainer's examples in `format_answer`'s tests before changing it.

### Piece 4

Only the example questions' first lines and the flow skill's one rule change; don't rewrite the skills. The skills
are shared by every tool's plugin (plugin/claude, codex, copilot, opencode): check how they are copied or linked
before editing.

### Piece 7

If piece 3 isn't merged yet, draw radios only; the checkboxes come with piece 3's component.

## Bringing it together

After the last merge: run `scripts/check.sh` once on the merged branch (with Node 24 on PATH), and try each piece's
Check: line there. Then walk it end to end in a dev build with a real Claude Code agent as Noyan, on a throwaway
project (its own OTAQ_HOME): open Edit flow, type "Add a security review after Validate" in the box, let Noyan ask a
long question with several answers allowed (ask it to), answer it from the box (expanded once), see the proposal
marked on the columns, ask for a change from the box, then Save flow. Also a ticket whose agent asks the brief's long
question: read and answer it from the Overview and from the Needs you strip. Report what you saw, with screenshots,
to the maintainer before anything is merged further.
