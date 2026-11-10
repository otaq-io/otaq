# UX rethink build · gap analysis (2026-11-10)

Checked against every record, not only the decision texts: `DECISIONS.md` (UX-1…61), `ASSUMPTIONS.md`, the
canvas "Otaq — New UX" (the build decisions UX-56…61 are "as drawn", and step 5 *is* the canvas),
`DESIGN-AGENT-BRIEF.md` 2b ("everything the design simply didn't draw stays as today", repeated in `QUESTIONS.md`),
and the other `ideas/` folders (no-fallback-agents G-numbers, no-skill-copies, noyan-intake, one-check-agent).

## Why questions-triage.md asked things already decided

It read only the decision texts and the assumptions. It skipped two sources that settle most of its "Open" list:
1. **The canvas.** Where a question was "should it be as the canvas draws it?", the canvas already answered it.
   In several places the triage recommended the opposite of the drawing (Idle's edge, the restart count, the PR
   title, "Not while you're looking", "Otaq can see").
2. **Brief 2b's rule: not drawn = stays as today.** Anything the canvas didn't draw and no decision removes is kept.

It also missed decisions outside `ux-rethink/`: G16 (no-fallback-agents) and UX-18's second notification ask.

## The two leftover branches

Both branch off `dcc4fef0` (main is one docs commit ahead, no conflicts expected). Worktrees still exist at
`.claude/worktrees/agent-a720397fa81ec87b0` and `agent-abe3cc9ea2f488715`.

- **`origin/ux-fix-contradictions`** (1 commit, 90 files): triage contradictions 1–11, the Done panel under Either,
  and UX-61's missing-skill warning back in the palette. One item goes against the canvas: **the List now starts at
  Backlog**. The canvas's List artboard (board-05) draws Backlog *after* the stages, then "Done · 3", which is what
  piece 16 built. Revert that part, or decide (below).
- **`origin/ux-cleanup`** (13 commits, 120 files): the approved dead-code removal (PROTOCOL_VERSION 12) and the
  "Not product" items. The "left" items are not blockers: `shell.cleanup` only remains in two doc comments
  (`switcher.rs:34` and its generated `RailBottom.ts`); the tools' own `permission_warning` is still used by
  Settings → Agents' tool-row warning (brief 2b keeps it), so it stays.
- check.sh (Node 24, 2026-11-10): **both green** on their own (3357 and 3335 tests). Merging one after the other
  has one conflict, `crates/otaq-core/src/flow/propose.rs` (both touched the flow proposal), so check.sh must run
  again after the second merge.

## The triage's Open list, re-sorted

### Answered by a decision (no question needed)

| # | Question | Answer | Source |
|---|---|---|---|
| 3 | Send-back reason posted as the user's comment | The agent's reason is the agent's, in History | UX-55 "History records the agent's reason and the user's answer" |
| 8 | Click-mode "To deliver" notifies | Under the "Ready for your review" kind | UX-28 |
| 13 | Old "Allow notifications?" banner | **Keep it**: it is UX-18's second ask, "once more at the first real agent question" (triage said drop) | UX-18 |
| 14 | "Answer here" leaves no History row | Record it (build differs) | UX-55 |
| 20 | "Rework" on a PR-open card | No: Send back, then Push to PR #N | canvas ticket-05 · 4, brief 2b |
| 21 | "Ask Noyan about this" | A new conversation; continuing is the switcher's | UX-47 |
| 27 | Edit ticket's layout | The same form, in place on Overview | A46, canvas ticket-06 · 2 |
| 56 | Template skill `security-check` | Settled with UX-61 (2026-11-09) | UX-61 |
| 57 | List vs board lift of needs-you cards | Answering never moves a row and both share one order, so the board's lift-on-answer goes too | UX-33 |
| 58 | "New agent on…" when Noyan's tool can't run | **Keep it** (the user's click, new conversation, only tools that can run); piece 7 removed it (build differs) | G16, UX-9 |

### Answered by the canvas

| # | Question | As drawn | Artboard |
|---|---|---|---|
| 4 | Idle's turquoise edge | **No edge**: Idle is calm (triage said keep turquoise; build differs) | board-03 |
| 10 | "Stop here" reads like undecided | It is meant to: "It waits in Validate until you choose" | flow-03 · 5 |
| 11 | Sign in… on a signed-out restart row | Reason only ("Codex isn't signed in · sign in first") | care-01 · 1 |
| 16 | Notification kinds as groups | Three toggles, as built | settings-01 · 4 |
| 18 | "Same as…" with several projects | One "Same as <project>" line, as built | flow-02, flow-01b · 7 |
| 22 | History names the agent | Yes: "Strong · 10:42", "Verifier · 10:40" (build differs) | ticket-03 · 4 |
| 23 | Template agents | The project's agent of that kind (Test in the browser · Verifier) | flow-02 |
| 32 | PR title starts with the key | **No key** in the PR title (triage said keep; build differs) | ticket-05 · 3 |
| 33 | ⇧⌘. resumes a paused project | Yes: "Pause / Resume ⇧⌘." | settings-03 Keyboard |
| 34 | Restart dialog count | **Resumable rows only**: "3 agents" over 4 rows, "Resume 3" (triage said all) | care-01 · 1 |
| 43 | Files picker ticket rows | "Strong's copy, working now" / "In Review" (build lacks the data; add it) | files-01 |
| 46 | "Pause <project>" in the palette | Yes: "Pause shop-web" | window-02 · 2 |
| 47 | "Not while you're looking at that ticket" | **A toggle**, as drawn (triage said a fixed sentence) | settings-01 · 4 |
| 48 | Keyboard list names | "Next that needs you", "Search", "Ask Noyan"… | settings-03 |
| 49 | Done order | Newest first | board-02, ticket-03 |
| 50 | "Not started" on Backlog cards | As built | board-03 |

### Answered by "not drawn = stays as today" (brief 2b)

| # | Question | Answer |
|---|---|---|
| 24 | "Write a plan" hidden with an empty Plan | Keep it reachable (Plan & helpers stay as today) |
| 25 | "Otaq can see" chips in Settings → Agents | **Keep** (2b lists them; triage said drop) |
| 35 | "Restart to update · stops N agents" | Keep |
| 39 / 40 | Box after ↵ / Esc keeps the draft | As today: closes; Esc keeps the draft |
| 45 | Palette recent items | Keep under the suggestions |
| 54 | Undo history lost on close | As built |

### Build differs from a record (to fix)

From the tables above: Idle edge (4), "Answer here" History row (14), History agent names (22), PR title without
the key (32), restart count (34), Files picker rows (43), "Not while you're looking" toggle (47), "Write a plan"
reachable (24), board's lift-on-answer (57), Noyan's New agent on… (58), and the List Backlog order on
`ux-fix-contradictions` if the canvas stands.

### Genuinely open (no record covers it), smallest first, with a default

These are small. Unless the maintainer objects, build the default:
- 2 · Existing installs' "Validator": rename a stored agent named Validator that only shipped flows use to Checker.
- 5 · Waiting card when the first stage is skipped: wait in the first stage the ticket will run.
- 6 · Automatic local delivery: plain message (no Review wait to write one in).
- 9 · Descriptions for existing projects' stored flows: fill stages of an unchanged shipped preset.
- 12 · Held-for-identity card: "Set name and email…" instead of Retry.
- 15 · ⌘↵ in the full form / review bar: the primary button.
- 17 · Repo-local git identity: global only.
- 19 · Card hints on the List: point at rows too.
- 26 · Skip + another agent: one combined line.
- 28 / 29 · Theme on upgrade: keep the user's choice; mode switch as built.
- 30 / 31 · Notification wording: "N tickets need you"; "SHOP-1 ready for your review" (the canvas's menu-bar wording).
- 36 · Settings view after restart: back to the Board.
- 37 · Changing Delivery with an Edit flow draft: keep the refusal.
- 38 · Project key: a read-only line in Repository.
- 41 · ⌥↵ new line in the box: keep.
- 42 · List waiting mark: the card's hourglass.
- 44 · Files source bar: "Showing main".
- 51–53 · Search: "No stage named …"; Esc clears the filter; accept dashes and quotes.

### One real conflict for the maintainer

- **The List's Backlog position.** UX-12 calls Backlog a fixed end of every flow and A40 says "flow order", which the
  fix branch read as Backlog first; the canvas draws Backlog after the stages (active work first). Recommendation:
  follow the canvas (revert that part of the fix branch). **Agreed 2026-11-10**: reverted on the fix branch's
  worktree (local commit, not pushed).
