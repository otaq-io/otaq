# Plan · ux-rethink

- **Ticket:** none
- **Date:** 2026-11-07
- **Decisions:** none in tuq/decisions (UX-1…UX-55 in ideas/ux-rethink/DECISIONS.md)
- **Tech spec:** none (no tech spec was written; ideas/ux-rethink/DECISIONS.md, GAP-REPORT.md, gaps/ and the code named below)
- **Orchestrator prompt:** tuq/prompts/ux-rethink-orchestrator.md

<!-- tuq:plan:start -->

The UX rethink (ideas/ux-rethink/DECISIONS.md, UX-1…UX-55; the design on the canvas "Otaq — New UX") is built in 29
pieces, one behaviour each. Before piece 1, main must hold the `no-skill-copies` branch (no skill copies, no fallback
agents, one agent per check stage) and `comment-attachments` (tuq 001–008): the redesign assumes both. Pieces 1–13 are
the groundwork the new screens need (delivery as a flow setting, no Ready for Dev column, stage descriptions, sending a
ticket back, hints, Create & start, agents without roles…); most run side by side. Pieces 14–26 are the new screens,
area by area, starting with the window shell every other screen sits in. Piece 27 (Edit flow in place) and piece 28
(first run) come last because they need the finished board and ticket; piece 29 updates the docs. The screens follow
the design agent's revised canvas: where a piece names an open point (R5, the flow-design gap, provider tools), the
maintainer decides it from that canvas before the piece starts. The agent in charge follows
tuq/prompts/ux-rethink-orchestrator.md.

## Pieces

1. Delivery is a setting of the flow, not a stage — waits for nothing
   UX-13, UX-23, UX-24, UX-25, UX-44, UX-51. A flow's delivery (local merge, PR or either; on Approve or automatic)
   becomes one flow-level setting in the flow file instead of a Delivery stage: crates/otaq-core/src/flow/model.rs
   (`Stage.delivery`, `StageKind`), flow/yaml.rs, flow/default.rs, flow/presets.rs and the shipped files in
   crates/otaq-core/flows/ (otaq-default.yaml, spec-first.yaml and the others). `column: with-previous` goes: every
   stage is its own column; old files that say it still load. The Delivery stage kind, the PR / merge stage kinds and
   Review's Approved sub-column go (crates/otaq-view/src/view_models/flow_columns.rs, card.rs, approve.rs; ui
   screens/board/Column.tsx, ApproveChoice.tsx). A migration (the next free number in crates/otaq-core/migrations
   after main's highest) moves every stored flow version's Delivery stage into the setting and puts tickets in the
   Delivery stage or Approved into Review states ("To deliver", "PR #N open"). Approve offers exactly what the flow's
   delivery allows, named by its action ("Approve & merge into main", "Approve & open PR"; both for "either"); "Done,
   nothing to merge" / "Done, don't merge" / "Merge into another ticket…" / "Push to PR #N" stay in its menu. Unchanged
   on purpose: Otaq never watches PRs (Mark merged / Mark closed stay, UX-23); a conflict or failed merge check, and an
   early conflict, go back to the nearest earlier work stage on their own and stop after three in a row (UX-24, UX-44;
   crates/otaq-core/src/review/mod.rs, review/early_conflict.rs). `otaq flow show` / `propose` read and write the new
   setting. One behaviour across core, the flow format, the view models and today's screens, on purpose.
   Check: core tests load each shipped flow with its delivery setting; an old file with a Delivery stage and
   `with-previous` loads; after the migration a ticket that was in Approved is in Review with its delivery click; an
   "either" flow offers both Approve actions; the board has no Approved section.
2. Starting a ticket puts it into the first stage — waits for 1
   UX-12, UX-15. The Ready for Dev column goes (crates/otaq-view/src/view_models/flow_columns.rs, board_page.rs,
   queue.rs, card.rs; core place/status where a ticket is "ready"). Start (the Backlog card's new Start button, a drag
   into the first stage, ⌘D) moves it into the first stage; a ticket that can't start yet (a dependency, no free agent,
   a flow change waiting) waits at the top of that column, its card saying why ("Waits for SHOP-12 to merge", "Waits
   for a free agent · 2nd in line", "Waiting for the flow change"), and starts by itself when it can. A migration moves
   tickets in Ready for Dev into this state. Backlog cards get a Start action (crates/otaq-view/src/view_models/card.rs,
   ui cards/TicketCard.tsx). Backlog and Done stay fixed at the ends of every flow.
   Check: a Backlog ticket started with no free agent shows at the top of Dev with "Waits for a free agent" and starts
   when one frees up; a ticket waiting for a parent starts after the parent merges; the board has no Ready for Dev.
3. Every agent reads what each stage is for — waits for nothing
   UX-53, UX-54. A stage's `why` (crates/otaq-core/src/flow/model.rs; today an editor-only note nothing reads)
   becomes the stage's description: what the stage settles and when it matters. `otaq prime` (crates/otaq-core/src/
   prime.rs) and `otaq prime --project` (Noyan) list every stage of the ticket's flow with its description. The shipped
   flows get a description per stage. Otaq's stage skills (plugin/skills/: work-ticket, product-spec, tech-spec,
   milestone-plan, validate, noyan) say that a stage with nothing to add for this ticket is finished at once with a
   short note ("Nothing to settle: README-only change"); that is an ordinary finish, never a skip (skips stay the
   user's, `options: [skip]`).
   Check: `otaq prime` on a ticket shows each stage's description; the shipped flows all have one; a flow without
   descriptions still loads.
4. An agent asks to send a ticket back to an earlier stage — waits for 3
   UX-32, UX-55. Any agent working a ticket (a work agent or a checker) that finds a problem belonging to an earlier
   stage, skipped or already run, asks the user, naming the stage and why: "Product gap: refunds over 30 days aren't
   specified. Send back to Product spec, or answer here?" A new form of `otaq ask human` (crates/otaq-cli/src/cli.rs)
   names the earlier stage; the question's two ways out are answering in place or sending back. On "send back" the
   ticket moves to that stage (a skip on it is cleared) and History (crates/otaq-view/src/view_models/history.rs)
   records the reason and the answer. The agent never moves a ticket back itself. Checks' "fails go to" and the
   conflict hand-back are unchanged. Skill text in plugin/skills/ says when to use it. One behaviour across CLI, core,
   the question view and History, on purpose.
   Check: an agent's send-back question shows as Needs you; "send back" moves the ticket to the named stage and clears
   its skip; "answer here" keeps it in place; an agent can't name a later stage.
5. A one-time hint shows once and never again — waits for nothing
   UX-14. A stored set of seen hints (crates/otaq-core/src/store/settings.rs), a request to read and dismiss one, and a
   generic dark popover with one button in the UI (crates/otaq-desktop/ui/src/components/). This piece wires no moment
   yet beyond a test hint: piece 28 places the first-run hints, and other pieces use it where their decision asks.
   Check: a hint shows, is dismissed, and doesn't show again after a restart; a fresh OTAQ_HOME shows it again.
6. Create & start in one step — waits for 2
   UX-15, UX-22. One request creates a ticket and starts it (crates/otaq-desktop/src/api/editor.rs, which today only
   creates; the start path of piece 2), with the start's refusals said in words (limits, setup failure). The full form
   gets "Create & start" next to "Create" (ui screens/editor/). Noyan never starts tickets (its skill and the service
   are unchanged).
   Check: Create & start puts the new ticket in the first stage (or waiting there with its reason); a refused start
   leaves the ticket in Backlog and says why.
7. Noyan's agent is one app setting, and roles go — waits for nothing
   UX-3, UX-9. The Project agent role (crates/otaq-core/src/agents/mod.rs `Role::ProjectAgent`, the Role field on the
   agent page, crates/otaq-view/src/view_models/agent_page.rs) becomes one app setting, "Noyan uses <agent>", with a
   per-conversation pick when starting a conversation. Role defaults and per-project agent overrides go; each stage
   names its agent and a ticket may pick another (UX-11). A migration keeps today's Noyan agent as the setting.
   Check: changing "Noyan uses" changes the next conversation's agent; a new conversation can pick another agent; no
   agent page shows a role.
8. "Ready for your review" can notify, off by default — waits for nothing
   UX-28. A new notification kind (crates/otaq-core/src/attention/notify.rs, needs_you.rs; Settings → Notifications in
   crates/otaq-view/src/view_models/settings_page.rs) for a ticket reaching your check; off by default. Everything else
   that notifies stays blocking-only, and "not while you're looking at that ticket" still applies.
   Check: with it off nothing notifies when a ticket reaches Review; turned on, one notification per ticket.
9. The theme follows the system — waits for nothing
   UX-16, A34. A theme mode (Match system / Light / Dark) with one light and one dark theme chosen, replacing the single
   stored theme (crates/otaq-view/src/theme.rs, crates/otaq-desktop/src/api/theme.rs, ui src/theme/theme.ts), a live
   listener for the OS appearance, and a migration that keeps today's theme as the matching choice. The tray icon and
   window follow it.
   Check: switching macOS to dark switches Otaq live on Match system; a stored light theme from before becomes the light
   pick.
10. Search lists tickets and filters by stage and agent — waits for 1, 2
   UX-40. `/` still filters the board in place (chips, counts; crates/otaq-view/src/view_models/search_page.rs,
   filter.rs, ui screens/board/Search.tsx) and also lists matching tickets with their status, Done included; ↵ opens
   one. New filters `in:<stage>` and `by:<agent>` (filter.rs parses only `is:`, `has:`, `dep:` today).
   Check: "in:review" leaves only Review's tickets; "by:Strong" those whose agent is Strong; a Done ticket appears in
   the list.
11. Tickets have no External ID — waits for nothing
   UX-46. Remove External ID from tickets: crates/otaq-core/src/board/tickets.rs, prime.rs (its line), board/fixes.rs,
   the editor (crates/otaq-view/src/view_models/editor_fields.rs, editor.rs, ui screens/editor/Fields.tsx), the PR
   dialog and History; a migration drops the column. Regenerate the ts-rs types.
   Check: no screen, CLI output or `prime` mentions an External ID; old tickets load.
12. Dependencies are set from the waiting ticket, also when creating it — waits for nothing
   UX-29, UX-39. The new-ticket form takes "Waits for" (crates/otaq-desktop/src/api/editor.rs, which can already link
   dependencies on create; the test asserting `deps == null` changes). Adding a link from the parent's side ("Blocks")
   goes (ui screens/dependencies/Dependencies.tsx, crates/otaq-view/src/view_models/dependencies.rs); Overview keeps
   "Waiting for it" read-only.
   Check: a ticket created with "Waits for SHOP-12" waits for it; Overview has no way to add a "Blocks" link.
13. Plain names everywhere — waits for 1
   UX-2, UX-4, UX-5, UX-34. Texts and view models only: Activity → Running; the ticket's Activity tab → History and
   Files tab → Changes (crates/otaq-view/src/view_models/sheet.rs); ticket Shell and project Terminals → Terminal;
   validator / verifier / check agent → checker; usage always "% left" (activity.rs, settings, limit banner).
   Check: no screen says Activity, Shell, Validator or "% used".
14. The window shell — waits for 13
   UX-8, UX-19, UX-49, A37–A39, A53. From the revised canvas (Window page): the rail becomes Board · Files · Settings
   with Terminal, Noyan and the app-settings gear at its foot (ui screens/shell/Rail.tsx); the project name switches
   projects and adds one, with a dot on a project that needs you (Header.tsx; the project menu's items move: Edit flow
   to the board, Project settings to the Settings view, Export flow to piece 21's Flow card); the title bar shows
   Running and Pause only while agents run, the usage chip only when it needs you; banners: limit (Resume <tool>,
   active once the reset time has passed, never a fallback), paused (with Resume), update ready (Restart to update,
   never a modal). ⌘P quick switcher stays.
   Check: with nothing running the title bar has no Running or Pause; a limit banner's Resume is active after the reset
   time; switching projects from the name works.
15. The board and its card — waits for 1, 2, 14
   UX-1, UX-14, UX-16, UX-21, UX-38, A6–A11, A41–A42. From the revised canvas (Board page): column headers say in
   plain words who acts; the card shows at most four lines plus one action (key, title, one status line, who and how
   long; a P1/P2 chip, a waiting line, an own-agent or skip line only when set), coloured edges only when it needs you
   (turquoise asks, magenta your review, red broke and waits); every state the app has keeps a card form (ideas/
   ux-rethink/DESIGN-AGENT-BRIEF.md 2b). The Needs you strip includes Your review and hides when empty. Board options:
   only Fold empty columns stays. Hovering a card highlights what waits for it; dragging a waiting ticket ahead is
   refused with a toast. Files: ui screens/board/, ui cards/TicketCard.tsx, crates/otaq-view/src/view_models/card.rs,
   needs_you_strip.rs.
   Check: the canvas's "ordinary day" board renders from fixture data; each card state in card_fixtures.rs fits four
   lines; the strip is gone with nothing needing you.
16. The List is a layout of the board — waits for 15
   UX-33, A40. List becomes a layout switch beside Board · Files · Settings; rows grouped by stage in flow order, inside
   each in the board's order; needs-you rows stand out only by colour (crates/otaq-view/src/view_models/list_view.rs,
   ui screens/list/).
   Check: answering a ticket in List changes its row, not its place.
17. Reviewing a ticket — waits for 1, 14
   UX-13, UX-42, A19–A21, A44. From the revised canvas (Ticket page): in Review the sheet opens on Overview (what the
   agent did, the checker, decisions, the ticket); the review bar on every tab with Approve named by its action and its
   menu; Local merge keeps a confirmation where commit messages can be edited, built so the user isn't blocked while
   they are prepared (the canvas's answer to UX-42); Approve & open PR opens the PR dialog; an open PR card offers Mark
   merged / Mark closed. Files: crates/otaq-view/src/view_models/sheet.rs, approve.rs, pr_dialog.rs; ui screens/sheet/,
   overview/, files/ReviewBar.tsx, pr/.
   Check: a Review ticket opens on Overview; Approve & merge shows the confirmation at once and merges; the PR card's
   Mark merged moves it to Done.
18. The ticket's tabs, History and editing — waits for 17
   UX-4, UX-43, UX-45, UX-47, A23–A24, A46. Changes (today's Files tab: diff, worktree tree, single-file view), History
   in plain sentences newest first with a quiet row per agent session and its id copyable, Session's switch named by
   stage (Agent · Validate · Run · Terminal), Edit ticket… turning Overview into the form, the ⋯ menu with "Ask Noyan
   about this", and the comment box keeping "tell the running agent now" as the canvas draws it (UX-43), with comment
   attachments (tuq 001–008). Files: crates/otaq-view/src/view_models/history.rs, session_tab.rs, sheet_actions.rs; ui
   screens/activity/, session/, editor/EditForm.tsx.
   Check: History shows each session's id and copies it; "Ask Noyan about this" starts a conversation about the ticket;
   a comment reaches the running agent only when chosen.
19. The box for new work — waits for 6, 15
   UX-22, UX-48, A25. One box docked at the bottom of Board and List ("New ticket, or ask Noyan…"): ↵ creates the ticket
   as typed (first line title, rest description), ⌘↵ creates and starts it (piece 6), ⇧↵ hands it to Noyan, which writes
   tickets to Backlog and opens its panel. It replaces quick create and the Ask bar (ui screens/editor/Modal.tsx quick
   create, screens/ask_bar/, crates/otaq-view/src/view_models/ask_bar.rs). ⌘N and the toolbar's New ticket focus it; in
   Files ⌘N stays "New ticket from this file". ⌘L still opens Noyan's panel.
   Check: ↵ creates in Backlog without an agent; ⌘↵ starts it; ⇧↵ opens Noyan's conversation with the text.
20. The full new-ticket form — waits for 11, 12, 19
   UX-11, UX-29, UX-31, UX-46, A26. "More options…" from the box opens the form with what was typed: title, description,
   priority (P3 default), Waits for, attachments (several, any type, per tuq 001–008), base branch (folded), and a folded
   "Agents for this ticket": per stage, "uses the stage's agent", another agent, or "Skip this stage" on stages whose
   flow lists `skip` (crates/otaq-view/src/view_models/stage_choices.rs). Create and Create & start.
   Check: a ticket created with an attachment, a dependency, another agent for Dev and a skip on a skippable stage shows
   each; a non-skippable stage offers no skip.
21. The project's settings page — waits for 1, 14
   UX-10, UX-25, UX-41, UX-52, A35. The project's Settings view is one page: Flow (its stages and agents, Edit flow,
   Export), Repository (base branch; the flow's delivery shown as the flow's and changed through it; merge check; the
   PR provider as the canvas proposes, brief 3b), Run, Workspace (setup command, the variables the user sets, files to
   copy, ticket resources, warm build), Notes for agents, Remove project. Today's project pages leave the app Settings
   (crates/otaq-view/src/view_models/settings_page.rs, ui screens/settings/Settings.tsx).
   Check: changing "Deliver as" on the page changes the flow's file (a new flow version); Export writes the flow YAML.
22. App settings for you and your machine — waits for 7, 8, 9, 14
   UX-10, UX-35, UX-36, UX-50, A30–A34. Seven topics: Agents (tools with their row actions, agents with Cancel / Save,
   "Noyan uses"), Limits & usage (the optional agent limit, off by default; usage % left), Notifications (piece 8's
   kind among the groups the canvas draws), Appearance (piece 9), Keyboard (the short list and all shortcuts), Data
   (data folder, Logs, Cleanup, usage-data consent), About. Projects leave the nav (crates/otaq-view/src/view_models/
   settings_page.rs, agent_tools_page.rs, agents.rs, agent_page.rs; ui screens/settings/).
   Check: each topic opens; an agent edit is kept only on Save; the agent limit is off on a fresh install.
23. Noyan's panel — waits for 7, 19
   UX-15, UX-47, A27–A29. Created tickets listed on top with Start on each (Noyan never starts them); continuing a
   conversation from the panel's switcher; the agent line from piece 7; copy that follows intake-only (no "splits", a
   proposal for two or more tickets). crates/otaq-view/src/view_models/agent_panel.rs, ui screens/agent_panel/.
   Check: Start on a created row starts that ticket; a conversation continues from the switcher.
24. Search popover and the palette's suggestions — waits for 10, 14
   UX-40, A47–A48. The search popover's ticket list (piece 10) in the title bar, and the palette's empty state led by
   "Next that needs you" (crates/otaq-view/src/view_models/palette.rs, ui screens/palette/).
   Check: an empty palette's first row jumps to the next ticket that needs you.
25. Restarts and upkeep — waits for 22
   A49–A53. After a restart, one dialog on open lists what stopped (ticked; a tool that can't resume unticked with its
   reason; Noyan conversations included; per-row results with Retry / Start from scratch); closed, stopped cards are
   red with Resume and in the strip (crates/otaq-view/src/view_models/after_restart.rs, banners.rs). Cleanup opens from
   Settings → Data (cleanup.rs; its groups and the uncommitted-changes warning stay). The menu-bar menu as the canvas
   draws it (status_icon.rs), with Restart to update keeping agents running.
   Check: after killing the service mid-session, opening Otaq shows the dialog; Cleanup opens from Data.
26. The Files view's copies — waits for 14
   UX-30, A36. The Showing picker lists the base branch, each ticket's copy and your worktrees, all as committed; your
   main folder appears as its checked-out branch (crates/otaq-view/src/view_models/files_view.rs; project_files.rs
   unchanged: never the files on disk). Filters, Same file, All changes, Go to file and Diff stay.
   Check: picking "your folder" shows its branch as committed; an uncommitted edit doesn't appear.
27. Editing the flow on the board — waits for 3, 15, 21
   UX-1, UX-12, UX-26, UX-41, UX-51, UX-53, A1–A5. From the revised canvas (brief 3a and 4, decided by the maintainer
   before this piece starts): Edit flow turns the board into the editor in place (Backlog and Done fixed; stages
   reordered, added, removed; each stage's kind, agent, skill with Pick a skill… / Ask the agent to write it, its
   description, skippable mark, fails go to, rounds); Start from… with the presets, Made for <project> and Import;
   Export; how a saved change reaches moving tickets as decided for R5. Replaces the overlay editor
   (ui screens/flow_editor/FlowEditor.tsx, crates/otaq-view/src/view_models/flow_editor.rs).
   Check: adding a check stage, saving, and starting a ticket runs it through the new stage; Import then Export gives
   back the same file.
28. A new user's first run — waits for 5, 6, 15, 17, 19, 22, 27
   UX-14, UX-16, UX-17, UX-18, UX-20, UX-35, UX-36, UX-37. Welcome (the model in one picture) → Tools (agent tools and
   git; Install and Sign in… in the row's terminal, as today) → Project (name, key, base branch; the folder-trust line
   and "can't be changed later" kept) → the board on the default flow, with "Your first ticket" in Backlog (Create &
   start / Not now). Hints (piece 5) at the first question, the first checker fail, the first Review (with Edit flow)
   and after the first approve, when the box for new work appears; notifications asked on the first Create & start;
   usage-data consent after the first approve; the permission-mode hint at the first permission prompt. Projects added
   later follow the canvas's answer to brief 3a. Files: ui screens/onboarding/, crates/otaq-view/src/view_models/
   onboarding.rs, onboarding_page.rs, empty_board.rs.
   Check: a fresh OTAQ_HOME walks Welcome → Tools → Project → board with the first-ticket card; Create & start asks for
   notifications; each hint shows once at its moment.
29. The README, docs and website match the new app — waits for 16, 18, 20, 23, 24, 25, 26, 28
   README.md, docs/how-it-works.md and website/ describe the new screens and names; new screenshots with the
   readme-screenshots skill.
   Check: every screen and name in the README matches the running app.

<!-- tuq:plan:end -->
