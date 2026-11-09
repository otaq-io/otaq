# Orchestrator · ux-rethink

- **Ticket:** none
- **Date:** 2026-11-07
- **Plan:** tuq/plans/ux-rethink.md
- **Tech spec:** none (no tech spec was written; ideas/ux-rethink/DECISIONS.md, GAP-REPORT.md, gaps/ and the code named in the plan)
- **Decisions:** none in tuq/decisions (UX-1…UX-55 in ideas/ux-rethink/DECISIONS.md)

The UX rethink: Otaq's screens rebuilt so a new user meets fewer concepts at once, following the canvas "Otaq — New
UX" (https://claude.ai/artifact/7tYuWTkYKLrq5gnUHSZWP1) and the decisions in ideas/ux-rethink/DECISIONS.md.

You are the agent in charge of building ux-rethink. You don't build the pieces: you start one agent per piece, check
what it hands back and merge it. Read the plan first; each piece's details there are its brief.

## Run order

<!-- tuq:order:start -->

Start a piece as soon as every piece it waits for is merged:

- First: 1 Delivery is a setting of the flow, not a stage, 3 Every agent reads what each stage is for, 5 A one-time hint shows once and never again, 7 Noyan's agent is one app setting, and roles go, 8 "Ready for your review" can notify, off by default, 9 The theme follows the system, 11 Tickets have no External ID, 12 Dependencies are set from the waiting ticket, also when creating it (side by side)
- After 1, 3: 2 Starting a ticket puts it into the first stage, 4 An agent asks to send a ticket back to an earlier stage, 13 Plain names everywhere (side by side)
- After 1, 2, 13: 6 Create & start in one step, 10 Search lists tickets and filters by stage and agent, 14 The window shell (side by side)
- After 1, 2, 7, 8, 9, 10, 14: 15 The board and its card, 17 Reviewing a ticket, 21 The project's settings page, 22 App settings for you and your machine, 24 Search popover and the palette's suggestions, 26 The Files view's copies (side by side)
- After 3, 6, 15, 17, 21, 22: 16 The List is a layout of the board, 18 The ticket's tabs, History and editing, 19 The box for new work, 25 Restarts and upkeep, 27 Editing the flow on the board (side by side)
- After 5, 6, 7, 11, 12, 15, 17, 19, 22, 27: 20 The full new-ticket form, 23 Noyan's panel, 28 A new user's first run (side by side)
- After 16, 18, 20, 23, 24, 25, 26, 28: 29 The README, docs and website match the new app

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

**Before piece 1:** main must already hold the `no-skill-copies` branch and `comment-attachments`. If either isn't
merged, stop and tell the user: every piece assumes no skill copies, no fallback agents, one agent per check stage and
several attachments per comment. Migrations take the next free number after main's highest at the time.

**Pieces 14–28 build screens from the revised canvas.** Before starting one, check that the user has the design agent's
revised canvas and has decided the open points the piece names (R5 for 27; the flow-design gap, brief 3a, for 27 and
28; provider tools, brief 3b, for 21; the non-blocking merge confirmation, UX-42, for 17). If not, ask the user and
start the groundwork pieces that don't wait meanwhile. Give each screen piece's agent the canvas link and the page name.

Every agent also gets: the decisions file ideas/ux-rethink/DECISIONS.md, and that code comments and tests name a rule
in words ("only the user skips a stage"), never by its UX number.

### Piece 1

The largest piece and the critical path. Watch that nothing still reads a Delivery stage (`grep -rn "StageKind::Delivery"
crates/`), that `otaq flow show | otaq flow propose --stdin` still round-trips, and that UX-23 / UX-24 really stay as
they are: no PR polling is added, and conflicts still go back without a click.

### Piece 4

The agent may only name an earlier stage of the ticket's own flow; the service refuses a later one or a stage the
ticket isn't past.

### Piece 7

Today's tool-problem "New agent on <model>" offer in Noyan's panel must not come back as a fallback: if the tool can't
run, the conversation waits (no fallback agents).

### Piece 27

Don't start it until the user has decided R5 (how a saved flow change reaches moving tickets, and check rounds) from the
revised canvas: the piece's details name the decision, not a default.

## Bringing it together

After the last merge: run the project's full tests (`scripts/check.sh`), try each piece's Check: line on the merged
branch, and walk the new user's path end to end on a fresh OTAQ_HOME with a real agent tool: Welcome → Tools → Project →
the board with "Your first ticket" → Create & start → the first question and its hint → Review → Approve & merge → the
box for new work appears. Then compare each page of the canvas with the running app, light and dark, and list what
differs for the user.
