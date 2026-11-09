# UX rethink · gap between the design and the app

2026-11-07. The design ("Otaq — New UX", 24 artboards) was compared with the app on branch `comment-attachments`
(main 44e8ad9d) by five reviewers, one per area. Their full reports, frame by frame with `path:line`, are in `gaps/`.
This file reads them against `DECISIONS.md` (UX-1…UX-22) and `ASSUMPTIONS.md` (A1…A53).

The goal behind the redesign is to introduce less at the start (P5, P6). So the design's drops are mostly deliberate,
and for each one the question is *when does it appear*, not *is it lost*.

## 1 · Earlier decisions the UX decisions overturn without saying so

**Decided 2026-11-07:** R1 → UX-23 (no PR watching), R2 → UX-24 (back to the agent), R3 → UX-25 (delivery in the
flow file), R4 → UX-26 (stage keeps its skill), R5 → sent to the design agent (DESIGN-AGENT-BRIEF.md). R6 → UX-28 (review notification, off by default). R7 → UX-29 (Waits for in the form). R8 → UX-30 (Files stays committed-only). R9 → UX-31 (skip only where the flow allows it). R10 → UX-33 (List keeps the board's order). UX-32 added (a later agent asks to undo a skip). R11 → left to the designer, with the plugins roadmap (DESIGN-AGENT-BRIEF.md). R12 → UX-34 (% left everywhere). Section 1 is done. Section 5 and the assumptions: QUESTIONS.md → UX-35…UX-55;
what the design only didn't draw stays as today (DESIGN-AGENT-BRIEF.md 2b).
In section 3, N1 and N2 are out for now (UX-27).

These go beyond the "known" overturns that DECISIONS.md lists. Each needs an explicit yes, and then the old rule is
recorded as replaced.

| # | UX decision | What it overturns | Size |
|---|---|---|---|
| R1 | UX-13 "moves to Done when merged / Done · not merged when closed" | Otaq never watches PRs: the user clicks Mark merged / Mark closed (`otaq-core/src/review/pr_state.rs:1`). Watching means polling `gh`/`az`. | M |
| R2 | UX-13 "Delivery problems put the card in Review with Needs you", A45 | A conflict or failed merge check goes back to the nearest Agent stage on its own, with restack, and stops after 3 in a row (FLW-17, d0120, `review/mod.rs:122`). | M |
| R3 | UX-13 + UX-10 "delivery" in project settings, A35 "Deliver as", A5 Solo autopilot | **Flow file says everything** (2026-11-06): delivery (`delivers: local/pr/either`, `mode: click/automatic`) is in the flow YAML today. With delivery in project settings, a shared flow no longer says how work leaves it, and Solo autopilot's "merges by itself" has no place in the flow. | M |
| R4 | A2/A3 stage panel without a skill | **No skill copies** (2026-11-05): a stage's `skill:` is the only place a skill is picked; the flow editor's "Pick a skill…" and "Ask the agent to write it" were agreed. The design drops both, and adds "Instructions" instead. How does a Spec first stage know to run product-spec? | S–M |
| R5 | A3 "after 3 tries in a row" fixed; A4 "new tickets use it at once"; A3 "removed stage → tickets move to the stage before"; no Apply now | Rounds are editable 1–5; a waiting flow change holds new tickets at the edge of the changed stretch (`flow/apply.rs:14`); Apply now defaults to the *next* stage (`apply_now.rs:12`). "New tickets use it at once" means tickets on different flow versions at the same time. | L |
| R6 | A33 / design "A ticket is ready for your review" notification | "Only blocking items notify; nothing else ever notifies" (`view_models/settings_page.rs:756`). | S |
| R7 | A26 Waits for in the new-ticket form, A42 | DPV-04: dependencies are set only in the sheet (`api/editor.rs:7`). Small; create can already link them. | S |
| R8 | A36 "Your folder, as it is on disk" | d0087: Files reads worktrees from git HEAD, "never the checkout's files on disk" (`board/project_files.rs:74`). | M |
| R9 | UX-11 (any agent per stage, no skip mentioned) | Today a ticket's choices are offered only where the flow declares options, and they include **skip**. "Only the user skips" (Noyan intake) needs a place to skip; UX-11 shows none. | S |
| R10 | A40 List "needs you first" | List keeps the board's order on purpose ("answering changes the row's edge, not its place", `list_view.rs:7`). | S |
| R11 | A12 gh/az under project Repository | gh/az are machine-wide installs and sign-ins; UX-10 puts machine things in app settings. | S |
| R12 | UX-19 / A32 usage | Design draws "% used" in Settings; the app's rule is "% left" everywhere. UX-19 itself says "15% left". | S |

## 2 · Wording in the decisions that breaks an agreed rule

- **UX-20 hint**: "Noyan … splits it into several tickets" invites the size-splitting that *Noyan is intake only*
  forbids. Suggest: "…or files separate tickets when it's several things."
- **Noyan mock** (noyan-01): Noyan creates two tickets with no proposal (N2: proposal first for two or more), asks
  "memory or Redis?" (N3: request questions only, never design), and diagnoses root causes (beyond a light look). Copy
  only, but it's the picture people will learn from.
- **onb-03**: "your folder and your current branch are never touched" is false: Local merge fast-forwards the base
  branch's clean checkout (`git/merge.rs:16`).
- **Welcome / first-review hint**: "Approve merges it into main" — true only when the project delivers by local merge.
- **care-02**: menu bar "Restart to update · stops 4 agents" contradicts A53's "working agents continue" and today's
  behaviour (a compatible update restarts only the app).
- **A28** "Reads code, never edits it" — Noyan also edits the flow when asked.

## 3 · New behaviour the design needs (not in the app at all)

| # | What | Where it comes from | Size |
|---|---|---|---|
| N1 | Live "what it's doing" line on cards and Overview ("writing tests", "editing 4 files", "running the tests"). Needs a tool-activity event from all four tools (`agent_tools/events.rs` has none), plus summarising. UX-21 only asks for "one status line"; the activity text is the drawing's. | board-02/03, ticket-02 | M–L |
| N2 | Answering permission prompts in the app (Deny / Allow once, A22). Each tool needs its own answer path; today it's "Answer in terminal" (`attention/needs_you.rs:789`). | ticket-02 | L |
| N3 | One-time hints with a "seen" store (UX-14, A16, A17). | board-01, ticket-01 | S |
| N4 | Starter ticket card in Backlog, Create & start / Not now (UX-17). | board-01 | S–M |
| N5 | Create & start in one call (UX-15, UX-22 ⌘↵) and Start on Noyan's created rows (A27). | new-01, noyan-01 | S |
| N6 | Theme follows the system: mode + one light + one dark theme, live OS listener (UX-16, A34). | settings-03 | S–M |
| N7 | Per-stage Instructions as a new flow YAML field (A3). | board-04 | S–M |
| N8 | Search as a ticket finder with `in:` / `by:` tokens (A47). | window-02 | M |
| N9 | Sign in without a visible terminal ("finish in your browser"). Some installs and logins ask for a password or device code, so the terminal still has to be reachable. | onb-02 | M |
| N10 | Notifications as 3–4 groups + "Not while you're looking" toggle (A33). | settings-01 | M |
| N11 | Agent limit always on with a default (A32); usage shown in Settings. | settings-01 | S |
| N12 | Environment variables read from `.env.local`, masked (A35). Possibly only illustration. | settings-02 | S |
| N13 | "Fixing · Validate found 2 problems" needs a problem count in the checker's verdict (A11), or drop the number. | board-03 | S |
| N14 | Noyan's agent as a global setting with a per-conversation switch (UX-9), replacing the Project agent role. | settings-01 | S–M |
| N15 | Agent editor with Cancel / Save instead of saving each change (A31). | settings-01 | S |

## 4 · Restructuring on what exists (most of the work, little new logic)

- **Window**: rail becomes Board · Files · Settings + Terminal, Noyan, gear (UX-8, UX-19); project name becomes a
  switcher (A38); project menu items move (Edit flow to the board, Project settings to the Settings tab; Export flow
  and Open in Finder need a home). Running/Pause only while agents run. (L)
- **Board**: Ready for Dev column folds into the first stage with waiting cards on top (UX-12); Approved sub-column
  and Delivery stage go (UX-13); Start on Backlog cards (UX-15); column subtitles saying who acts (UX-16); four-line
  card (UX-21); strip regrouped and hidden when empty (A9); red edge for "broke" (A10); List as a layout switch (A40);
  dependency hover line and toast (A41–42). (L)
- **Edit flow in place** on the board (A1–A5), replacing the overlay editor (`FlowEditor.tsx`, `flow_editor.rs`). (L)
- **Onboarding** to three steps (UX-16), Welcome as the model picture. (M)
- **Ticket sheet**: opens on Overview in Review (A20); tabs renamed (UX-4); Approve names its action (UX-13); details
  line / STAGES / Attachments / Move… leave; Edit ticket… in place (A46). (M)
- **One box for new work** replacing quick create and the Ask bar (UX-22, A25–26). (M)
- **Settings**: project settings become the project's Settings tab, one page (UX-10, A35); App settings regrouped to
  seven topics, Agent tools + Agents merged (A31), limits + usage merged, Cleanup into Data. (M–L)
- **Restart**: banner + sheet become a dialog on open (A49); update banner on the board (A53). (S)
- **Palette** "Suggested" empty state (A48); Running popover renames (A37). (S)

## 5 · Dropped by the design with no decision — keep, move or drop?

Grouped. Items marked ⚠ look like real losses (safety, consent, recovery, or the design contradicts itself).

- **Onboarding**: ⚠ folder-trust disclosure ("their hooks and settings will run"); ⚠ "Share usage data" consent; ⚠
  "Can't be changed later" on the key; Claude Code permission-mode warning; install command visible before clicking;
  "already in Otaq" error; folder drag-and-drop; the agent proposing a flow ("Made for <project>").
- **Board / card**: card states never drawn — check out of rounds (Decide), Idle, Held (base diverged), Parent closed,
  Resume failed / Start from scratch, Merging · merge check (Stop), Merged into another ticket, Parked · has work,
  plan limit stop; piece rows; hover facts and quick actions; Resume all; Group by Status / Agent; Dependencies
  "Always"; adding a link from the parent's side ("Blocks"); Fold empty columns.
- **Edit flow**: Import / Export flow; `column: with-previous`; why line; rounds editor; Cancel a waiting change.
- **Ticket**: ⚠ comment attachments (this branch, tuq 001–008: the design predates them); ⚠ History's session ids
  (needed to resume, REC-03); Local merge confirmation (squash message, "runs npm test first"); "Merge into another
  ticket…" and "Push to PR #N" (UX-13 names the first as later); early "Conflicts with main"; worktree tree and single
  file view on Changes; History filters; helpers' terminals in Session; base branch and External ID in the form; PR
  proposal by skill and editable commit messages.
- **New work / Noyan**: ask Noyan about the selected ticket; Continue a conversation from the bar; "New reply"; Noyan
  panel's CHANGED rows, Delete, Show on board; the tool-problem callout.
- **Files**: ⚠ Same file filter (two tickets editing one file); All changes; Go to file; Diff; Copy path; per-line
  ticket bars; other user worktrees.
- **Settings**: ⚠ Logs folder; ⚠ Theme credits (licence obligation); Tools page (git/gh/Node checks); tool row's Sign
  in…, Hide, Re-check, Disconnect, "Otaq can see"; agent Effort; "What this agent reads"; Duplicate, default model;
  back-pressure ("don't start while Needs you has N"); master switch and Play a sound; Terminal renderer fallback; full
  keyboard list and the single-letter switch; project Name editing; "Let finished tickets answer questions"; template
  refresh and schema paths; Run's "Ask the agent".
- **Restarts & upkeep**: ⚠ Noyan conversations in the restart dialog; per-row Retry / Start from scratch; ⚠ Cleanup's
  "Done tickets with uncommitted changes" warning; warm builds in Cleanup; menu bar "Your checks and stopped tickets";
  the strip's own Interrupted group (the design's frame doesn't list the stopped card it describes).

## 6 · The app breaks agreed rules today, whatever the design says

Agreed but not built (on branch `no-skill-copies`, `../otaq-no-skill-copies/ideas/`):

- **No fallback agents**: "Use fallback" card actions (`view_models/card.rs:349`), "Use fallback now" on the limit
  banner (`limit_banner.rs`), "New agent on Sonnet" in the agent panel's tool-problem callout (`agent_panel.rs:20`).
- **One agent per check stage**: several check agents per stage (d0130, `card_page.rs:51`, `FlowEditor.tsx:805`).
- **No skill copies**: `write_skill.rs`, skill import in `agents/mod.rs`.

The design already assumes all three, so the redesign should land after them or include them.

## 7 · Already built — look or wording only

Pause and its scope menu; Terminal drawer; Apps rows; question box with options; decisions with Change…; ⋯ menu items
and their disabled reasons; first-close note; quit question; Cleanup's nothing-ticked rule; About's update and PATH
rows; notification ask before the OS prompt; turquoise card, strip, rail dot and ⌘J; Plan one-line card form; PR
dialog fields; toast after merge; Send back dialog; dependency waiting and its card line.
