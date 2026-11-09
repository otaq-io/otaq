# UX rethink · order of the work (skeleton)

2026-11-07. A skeleton, written before the revised canvas: which areas come first and what waits for what. It is not
yet the milestone plan. When the design agent's revised canvas is back, phases 3–4 get their detailed pieces
(the usual `tuq/plans/` plan and orchestrator prompt). Decisions: `DECISIONS.md` (UX-1…UX-55).

Sizes: S (a day or less of agent work, one area), M (several areas or one big one), L (split further when planned).

## Phase 0 · Land what's in flight (first, nothing else starts before it)

The redesign assumes all of this.

- **0a · Merge `no-skill-copies` into main.** 102 commits ahead of main: no skill copies, no fallback agents (the card's
  Use fallback and the limit banner's Use fallback now are already gone there), one agent per check stage, skips and
  "required" (G18–G21). Check that the no-fallback and one-check-agent records (marked "not built yet") are really
  done on the branch; whatever isn't, finish there first. Also removes the agent panel's "New agent on Sonnet" if it
  is still there.
- **0b · Finish and merge `comment-attachments`** (tuq 001–008, this branch). The new design reuses its comment box in
  the full new-ticket form (A26), Send back and every comment box.

## Phase 1 · Backend and view-model pieces that don't wait for the drawings

All start after phase 0 and run side by side unless a "waits for" says otherwise. Each keeps today's screens working
(the old UI shows the new data until phase 3 replaces it).

| # | Piece | Decisions | Waits for | Size |
|---|---|---|---|---|
| 1.1 | **Delivery becomes a setting of the flow, not a stage.** The flow file gets a flow-level delivery (local / PR / either; on Approve / automatic); the Delivery stage kind, the PR / merge stage kinds and the Approved sub-column go; `column: with-previous` goes (every stage its own column). Migration: stored flow versions and shipped presets move their Delivery stage into the flow setting; tickets in the Delivery stage or Approved become Review states ("To deliver", "PR #N open"). Approve offers what the flow's delivery allows. Mark merged / Mark closed stay; conflicts and failed merge checks still go back on their own. | UX-13, UX-23, UX-24, UX-25, UX-44, UX-51 | 0a | L |
| 1.2 | **Ready for Dev goes as a column.** Start moves a ticket into the first stage; one that can't start yet waits at the top of that column with its reason (a dependency, a free agent, the flow change). Every Backlog card gets Start. | UX-12, UX-15 | 0a | M |
| 1.3 | **Each stage's description, read by every agent.** `why` becomes the stage's description; `prime` and `prime --project` give agents and Noyan every stage's description. Skills say that finishing a stage at once is fine when it has nothing to add. | UX-53, UX-54 | 0a | S |
| 1.4 | **An agent asks to send a ticket back to an earlier stage.** A question with the stage and the reason; the user's "send back" moves the ticket there (turning a skipped stage back on); History records both. A CLI form for agents and skill text for every stage's skill. | UX-32, UX-55 | 1.3 | M |
| 1.5 | **One-time hints.** A stored "seen" set per hint and an API to read and dismiss them; the first-run moments (first question, first checker fail, first Review, after the first approve), the usage-data banner after the first approve, and the permission-mode hint at the first permission prompt. | UX-14, UX-17, UX-18, UX-20, UX-35, UX-36 | 0a | S |
| 1.6 | **Create & start in one call,** with the start's refusals said; Start on Noyan's created rows. | UX-15, UX-22, A27 | 1.2 | S |
| 1.7 | **Agents without roles.** Noyan's agent becomes one app setting with a per-conversation switch; the Project agent role, role defaults and per-project agent overrides go. | UX-3, UX-9 | 0a | M |
| 1.8 | **Notifications:** a "Ready for your review" kind, off by default. | UX-28 | 0a | S |
| 1.9 | **Theme follows the system:** mode (system / light / dark) with one light and one dark theme, the stored theme migrated, a live OS listener. | UX-16, A34 | — | S–M |
| 1.10 | **Search filters:** `in:<stage>` and `by:<agent>`, and a list of matching tickets (Done included) beside today's in-place filter. | UX-40 | 1.1 | S–M |
| 1.11 | **Removals:** External ID; Group by Status / Agent and Dependencies "Always"; adding a dependency from the parent's side. Dependencies can be set in the new-ticket form. | UX-29, UX-38, UX-39, UX-46 | 0a | S |
| 1.12 | **Names:** Activity → Running, the ticket's Activity → History and Files → Changes, Shell and Terminals → Terminal, check agent / validator / verifier → checker; usage always "% left". Texts and view models only. | UX-2, UX-4, UX-5, UX-34 | 1.1 | S |

## Phase 2 · Waits for the design agent (the brief, `DESIGN-AGENT-BRIEF.md`)

Nothing is built here until the revised canvas answers it and the maintainer decides.

- **2a · When a flow change reaches moving tickets, and check rounds** (R5, brief section 4). Decides whether today's
  waiting change, Apply now and Cancel change stay as they are.
- **2b · How a project gets its flow, and designing your own** (brief 3a): Flow setup for later projects, where "Made
  for <project>" goes, the in-place editor, Import / Export placement.
- **2c · The stage panel's fields:** description vs "Instructions" (one field or two).
- **2d · A merge confirmation that doesn't block** (UX-42) and how "Also tell the agent now" looks (UX-43).
- **2e · Provider tools and plugins** (brief 3b). The plugin system itself is its own roadmap item, outside this work.

## Phase 3 · The new UI, area by area (after the revised canvas)

Each area is planned in detail from the revised canvas. Order and what each waits for:

| # | Area | Waits for | Size |
|---|---|---|---|
| 3.1 | **Window shell:** rail (Board · Files · Settings, Terminal, Noyan, gear), project-name switcher with dots, title bar (Running and Pause only while agents run, usage chip only when it needs you), limit and paused banners, update banner. Everything else sits inside it. | 1.12 | L |
| 3.2 | **Board and card:** column headers saying who acts, the four-line card with one action and its edges, the strip (hidden when empty, Your review in it), waiting cards on top of the first stage, List as a layout switch in the board's order, Fold empty columns, dependency hover line and toast. | 1.1, 1.2, 3.1 | L |
| 3.3 | **Ticket sheet:** Overview first in Review, Approve that names its action and its menu, Changes / History (with session rows), Edit ticket in place, ⋯ with "Ask Noyan about this", PR card with Mark merged / Mark closed, the merge confirmation (2d). | 1.1, 3.1, 2d | M–L |
| 3.4 | **New work:** the box on Board and List (↵ / ⌘↵ / ⇧↵), More options… form (priority, Waits for, attachments, base branch, the ticket's agents and skips). | 0b, 1.6, 1.11, 3.2 | M |
| 3.5 | **Project settings** as the project's Settings tab, one page: Flow card (Edit flow, Export), Repository (base branch, the flow's delivery, merge check), Run, Workspace, Notes, Remove. | 1.1, 3.1, 2b, 2e | M |
| 3.6 | **App settings:** seven topics; one Agents page (tools, agents with Cancel / Save, Noyan's agent); Limits & usage (optional limit, % left); Notifications; Appearance; Keyboard (short list + all); Data (with Cleanup); About. | 1.7, 1.8, 1.9, 3.1 | M |
| 3.7 | **Noyan panel:** created rows with Start, continuing from the switcher, copy that follows intake-only. | 1.6, 1.7, 3.1 | S |
| 3.8 | **Search popover and the palette's Suggested list.** | 1.10, 3.1 | S |
| 3.9 | **Restarts and upkeep:** restart dialog on open, Cleanup from Settings → Data, menu bar. | 3.6 | S |
| 3.10 | **Files view:** the Showing picker's labels (your folder as committed). | 3.1 | S |

## Phase 4 · Last

- **4.1 · Edit flow in place on the board, and the flow-change rules** decided in 2a/2b. The largest UI piece. Waits
  for 1.1, 1.3, 2a, 2b, 2c, 3.2.
- **4.2 · First run:** Welcome (the model in one picture), Tools (terminal under each row, as today), Project, then the
  board with the "Your first ticket" card; the hints (1.5) wired to their moments; the box for new work appearing after
  the first approve. Last on purpose: it shows the finished board, card, ticket and box. Waits for 1.5, 1.6, 3.2, 3.3,
  3.4, 2b.
- **4.3 · README, docs, website and screenshots** (the `readme-screenshots` skill). Waits for everything.

## The critical path

0a → 1.1 → 3.2 → 3.4 → 4.2, with 3.1 alongside phase 1 once 1.12 is in. The design agent's revision (phase 2) runs in
parallel with phases 0–1 and gates 3.3, 3.5 and 4.1.
