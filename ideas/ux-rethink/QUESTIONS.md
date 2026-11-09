# UX rethink · remaining questions (2026-11-07)

Only the gaps where the design, or a UX decision, conflicts with what the app does or an earlier rule. Everything the
design simply didn't draw stays as today (DESIGN-AGENT-BRIEF.md, section 2b). Each has a recommendation.

## Onboarding
- Q1 · **→ UX-35** · Usage-data consent. Asked once at the end of first run today; UX-16's first run has no place for it.
  Rec: ask once, as a quiet one-time banner after the first ticket is approved; the toggle lives in Settings → Data.
- Q2 · **→ UX-36** · Claude Code permission-mode warning ("with many agents, prompts will stop them often"). On the Tools step today.
  Rec: a one-time hint at the first permission prompt (UX-14).
- Q3 · **→ open gap for the designer** · The agent proposing a flow ("Made for <project>"), and projects added later. UX-16 drops the Flow step.
  Rec: every new project starts on the default flow, first run or not; the agent proposal moves into Edit flow's
  Start from….
- Q4 · **→ UX-37 (terminal as today)** · Sign in without a terminal (onb-02). Some installs and sign-ins ask for a password or a device code.
  Rec: "Signing in…" progress, with the terminal one click away (Show terminal).

## Board
- Q5 · **→ UX-38** · Board options: Group by Status / Agent, Dependencies "Always", Fold empty columns.
  Rec: drop Group by Status / Agent and Dependencies "Always" (calm board, UX-14; hover shows links); keep Fold empty
  columns.
- Q6 · **→ UX-39** · Adding a dependency from the parent's side ("Blocks"). The design has only "Waits for".
  Rec: drop; Overview keeps "Waiting for it" as a read-only list.
- Q7 · **→ UX-40** · Search (/). Today it filters the board in place; the design shows a list of matching tickets.
  Rec: both: typing filters the board as today, and a popover lists matches (Done included); ↵ opens one. Add `in:`
  and `by:`.

## Edit flow
- Q8 · **→ UX-41 (keep, essential)** · Import / Export a flow. The design drops them, but flows are meant to be shared as files.
  Rec: keep: Import under Start from…, Export in Edit flow's bar and on the project's Flow card.
- Q9 · **→ UX-51** · Two stages in one column (`column: with-previous`). UX-1 says the columns are the stages.
  Rec: drop; every stage is its own column. (One-check-agent's split copies would then get their own columns.)
- Q10 · **→ open: see below** · A stage's "why" line. Not in the design's stage panel.
  Rec: drop.

## Ticket
- Q11 · **→ UX-42 (non-blocking: designer)** · Local merge confirmation (editable squash message, "runs the merge check first"). The design merges on the
  "Approve & merge into main" click.
  Rec: keep the confirmation.
- Q12 · **→ UX-43 (designer draws it)** · Comments and the running agent. Today a comment is for the next stage unless you tick "Also tell the agent
  now"; the design says every comment reaches the agent when it's idle.
  Rec: keep today's tick.
- Q13 · **→ UX-44** · Early "Conflicts with main" before Approve. UX-21 keeps conflict details off the card.
  Rec: show it on Overview only; the card speaks only when delivery needs you.
- Q14 · **→ UX-45** · Session ids in History (to resume a tool's session by hand). A24 makes History plain sentences.
  Rec: keep a quiet session row per run with its id copyable.
- Q15 · **→ UX-46 (External ID dropped)** · A ticket's base branch and External ID. Not in the design's form.
  Rec: keep both, folded in More options… and Edit ticket.

## New work and Noyan
- Q16 · **→ UX-47** · Asking Noyan about a ticket, and continuing a conversation. The Ask bar did both; UX-22 removes it.
  Rec: continue from the panel's switcher (⌘L); "Ask Noyan about this" in the ticket's ⋯ menu.
- Q17 · **→ UX-48** · ⌘N in Files. Today it is "New ticket from this file"; UX-19 makes ⌘N focus the box for new work.
  Rec: the box is on Board and List only; in Files ⌘N stays "New ticket from this file".

## Settings and window
- Q18 · **→ UX-49** · Limit banner without Resume. Today Resume <tool> is always the user's click.
  Rec: keep Resume <tool> on the banner (no fallback), active once the reset time has passed.
- Q19 · **→ UX-50** · Agent limit always on (A32). Today it's off by default; limits are a "later" concept (step 1).
  Rec: keep it optional, off by default.
- Q20 · **→ UX-52** · Environment variables "from .env.local", masked (settings-02).
  Rec: drop; list only the variables you set.
