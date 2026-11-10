# Plan · release-fixes

- **Ticket:** none
- **Date:** 2026-11-10
- **Decisions:** 009, 010; UX-11, UX-14, UX-16, UX-21, UX-26, UX-36, UX-55, UX-56, UX-58, UX-60, A20, A22, A42, A49, A52 in ideas/ux-rethink/; S5–S7 in ideas/no-skill-copies/
- **Tech spec:** none (fixes to agreed behaviour; the evidence is in the Windows release test, linked below)
- **Orchestrator prompt:** tuq/prompts/release-fixes-orchestrator.md

<!-- tuq:plan:start -->

The Windows release test of ux-rethink-fixes (PR #61, 2026-11-10) found 23 places where the app doesn't do what was
agreed. The maintainer chose to fix all of them before the release, then re-test on Windows. Each piece is one
behaviour, most of them bugs against a decision that already says what should happen; 009 and 010 are the two new
decisions taken while reading the results. The first seven pieces are the blockers. Nearly everything runs side by
side; only the restart dialog's row waits for the signed-out detection, and the two Edit flow additions wait for the
blank-editor fix because they touch the same screen. Every piece's evidence (what was clicked, what was seen, the
screenshot) is in the test results, ideas/ux-rethink/build/release-test/RESULTS-A.md … RESULTS-D.md (phase letter given per
piece; the screenshots they name are in that folder's shots/). The agent in charge follows tuq/prompts/release-fixes-orchestrator.md.

## Pieces

1. Edit flow never goes blank after a refused reorder — waits for nothing
   RESULTS-D (A1). On shop, dragging Validate in front of Dev in Edit flow was refused with no message; then stage
   panels stopped opening, and after Cancel, every Edit flow on that project showed an empty main area (no board, no
   bar, no error), also after restarting the app and the service, until Settings → Flow ⋯ → Use a preset replaced the
   flow. Find the cause (a page error from the stored draft or flow is likely; crates/otaq-desktop/ui/src/screens/flow_board/,
   crates/otaq-view/src/view_models/flow_board.rs, crates/otaq-core/src/flow/draft.rs). Fix it so the editor always
   draws; a refused drag says why in plain words; a broken stored draft is dropped or repaired, never left to blank
   the editor.
   Check: a test replays the refused reorder and the editor's view still builds; the page draws the editor for that
   state; dragging a check before the first agent stage shows its reason.
2. A logged-out tool shows as signed out — waits for nothing
   RESULTS-A (UX-16, UX-37). Claude Code in the VM was logged out (`claude auth status` → loggedIn false) but Tools
   and Settings → Agents said "signed in", even after Re-check, so the first ticket failed at once with "Login
   expired". Make the sign-in state match the tool's own answer for every tool (crates/otaq-core/src/agent_tools/*/,
   claude/mod.rs `signed_in`, connected.rs), so a logged-out tool shows Sign in… on Tools and in Settings → Agents.
   Check: with a tool reporting logged out, Tools and Settings → Agents show it signed out with Sign in…; Re-check
   updates it.
3. A card says why its tool stopped — waits for nothing
   RESULTS-A (UX-14, UX-21, A11). After Claude Code's login failure, SHOP-1's card said only "Idle" with Answer, while
   the notification said "stopped on an error · Login expired · Please run /login". When an agent's tool stops on an
   error, the card's status line names it ("Stopped · login expired") with the red edge (broke and waits), and Overview
   says what to do (sign in, then Resume).
   Check: a session ending on a tool error shows the reason on the card, red, in Needs you, and Overview's next step.
4. The restart dialog lists a tool that can't resume — waits for 2
   RESULTS-D (A49, canvas care-01 · 1). After a restart, a stopped agent on a signed-out Claude Code got no row: the
   dialog said "2 agents were stopped by a restart" with no rows and "Resume 0". Each stopped agent has a row; one
   whose tool can't run is unticked with its reason ("Claude Code isn't signed in · sign in first"); the title counts
   the agents that can resume and the button matches ("Resume 3"), as drawn
   (crates/otaq-view/src/view_models/after_restart.rs, ui/src/screens/recovery/AfterRestart.tsx).
   Check: a restart with one resumable and one signed-out agent shows two rows, one unticked with the reason, title
   and button "1".
5. The merge message writer asks in the Approve dialog, and stops with the ticket — waits for nothing
   Decision 010, UX-56; RESULTS-C. With Codex, the delivery agent's message session stopped at a permission prompt
   shown nowhere, so 5 of 5 Approve dialogs said "Developer is still writing…"; it also kept running after the merge
   (Running listed the Done ticket) and left its worktree. Build 010: Review stays calm; Approve & merge / open PR say
   when the writer waits for permission, with Answer in terminal beside the plain message; the writer's session ends
   when the ticket is delivered, closed or sent back, and its worktree goes with the usual cleanup.
   Check: a writer session waiting on a prompt shows the dialog's line and Answer in terminal; merging with the plain
   message ends the session and Running no longer lists the ticket; no worktree is left.
6. Otaq's notes and "Tell <agent> now" reach a Codex agent — waits for nothing
   RESULTS-A, RESULTS-C (UX-58, UX-43). With Codex on Windows the footer said "Unsent text in the input line · Otaq's
   note waits until it's empty" while Codex's input held only its placeholder ("Ask Codex to do anything"), so every
   told comment and Otaq's own notes needed Deliver now → Clear it and send
   (crates/otaq-view/src/view_models/terminal_strip.rs and the core's input-line reading). An empty input line with
   the tool's placeholder counts as empty for every tool.
   Check: a test with Codex's placeholder on screen finds the line empty; a told comment reaches an idle Codex agent
   by itself.
7. Personal Codex skills are found — waits for nothing
   RESULTS-D (no-skill-copies S5, S6). A skill in %USERPROFILE%\.codex\skills\<name>\SKILL.md (CODEX_HOME unset) was
   listed as "Personal · Not committed yet: no tool has it", the stage warned "You don't have the skill", and a Codex
   ticket reaching it was held with "Link the skill into Codex's skills folder…" (Check again kept it held). Codex's
   personal skills folder is read on every platform, Windows included (crates/otaq-core/src/agent_tools/codex/,
   flow_editor.rs:976).
   Check: a test with a personal Codex skill lists it as Codex's, the stage has no warning and a ticket starts there.
8. Removing a stage waits for the tickets in it, held ones too — waits for nothing
   RESULTS-D (UX-60). Removing Update the docs while SHOP-8 was held in it (missing skill) said "No ticket is between
   Update the docs and Dev now, so it applies at once"; after Save SHOP-8 was moved to Dev without asking and still
   showed the old hold. A ticket held in a stage counts as in it: the change waits ("changing", Cancel change, Apply
   now… asking where it goes), and a ticket that moves loses the hold of the stage it left.
   Check: a test removes a stage holding a held ticket: the change waits; Apply now asks; the moved ticket has no old
   hold.
9. A check that runs out of rounds can always be sent on — waits for nothing
   Decision 009, UX-60; RESULTS-C. Decide offered "Send it on to Code review" on notes' Validate and then refused it
   ("Validate can't be skipped: the flow doesn't mark it skippable"). Send it on works on every check; "Skip this
   check" for a limit wait stays skippable-only. Also the card's words: "Validate failed 1 time · decide" (singular),
   crates/otaq-view/src/view_models/card_line.rs:275.
   Check: a test runs a required check out of rounds and Send it on moves the ticket on with the last report; the
   card reads "failed 1 time" with rounds 1.
10. "Ask the agent to write it" in Pick a skill… and the missing-skill warning — waits for 1
   RESULTS-D (UX-26, S7, canvas flow-01b · 5). The core has it (crates/otaq-core/src/flow/write_skill.rs, draft.rs
   FED-06) but Pick a skill… and the stage's missing-skill warning offer only "Use <name>". Add it to both, as drawn.
   Check: choosing it on a stage starts the writer as S7 says and the stage shows it being written.
11. The "drafting a flow" line goes when the draft does — waits for 1
   RESULTS-D. "Shop repo flow is drafting a flow for shop… Show" stayed in the editor after the suggestion was
   discarded (and after Save) while Noyan's agent was idle (crates/otaq-view/src/view_models/flow_board.rs:837).
   Check: after Discard or Save, the line is gone.
12. Tools shows no permission warnings — waits for nothing
   RESULTS-A (UX-36, canvas onb-02). The Tools step showed amber "Your Codex/GitHub Copilot settings ask for
   permission prompts…" warnings; UX-36 moved them to the one-time hint at the first permission prompt. Remove them
   from Tools only (Settings → Agents keeps a tool row's warning, brief 2b).
   Check: the Tools step's view for a tool with a permission warning shows none; Settings → Agents still does.
13. Windows notifications are Otaq's — waits for nothing
   RESULTS-A (Windows). Every toast was titled "Windows PowerShell" with PowerShell's icon. Toasts carry Otaq's name
   and icon on Windows (the installed app's identity), on the NSIS install.
   Check: on Windows, a notification from the installed app shows "Otaq" and its icon.
14. Quit Otaq from the tray asks while agents work — waits for nothing
   RESULTS-D (A52, canvas care-02 · 3). On Windows, the tray's Quit Otaq with "1 agent running" quit at once with no
   "Keep them running / Quit and stop agents" question (crates/otaq-desktop/src/door.rs:633/1151, view_models/quit.rs
   describe it). The tray's Quit asks the same as the window's, on every platform.
   Check: with an agent working, the tray's Quit Otaq shows the question; Keep them running leaves the agent working.
15. A ticket's own agent shows from the moment it's set — waits for nothing
   RESULTS-B (UX-11, UX-21). SHOP-5 (Dev set to Checker) showed no own-agent line in Backlog, waiting, or on Overview;
   only once Dev started ("Dev: Checker instead of Developer"). Show the line on the card and Overview whenever it is
   set, from Backlog on.
   Check: a Backlog ticket with Dev set to another agent shows the line on its card and on Overview.
16. A refused drag says what the ticket waits for — waits for nothing
   RESULTS-B (A42, canvas board-05 · 4). Dragging a waiting ticket ahead showed only "It starts by itself when it
   can, in priority order" (crates/otaq-core/src/board/moves.rs:144). Say what it waits for: "SHOP-30 can't go to
   Validate yet · It waits for SHOP-27 to merge" (or a free agent).
   Check: dragging a ticket that waits for another names that ticket in the toast.
17. The palette says "Pause <project>" with its shortcut — waits for nothing
   RESULTS-B (canvas window-02 · 2). The palette shows "Pause this project" with no shortcut; drawn "Pause shop-web
   ⇧⌘." (Ctrl+Shift+. on Windows) (crates/otaq-view/src/view_models/palette.rs).
   Check: the palette's suggestion reads "Pause <project name>" with its shortcut.
18. Overview shows the command a permission prompt asks for — waits for nothing
   RESULTS-C (A22, UX-27, canvas ticket-02 · 2). Overview showed only "Answer in terminal"; drawn: the command and
   Answer in terminal (Otaq still doesn't answer prompts).
   Check: a ticket at a permission prompt shows the command on Overview above Answer in terminal.
19. "What the agent did" shows the latest round — waits for nothing
   RESULTS-A, RESULTS-C (A20). After a send back or a conflict fix, Review's Overview still showed the first round's
   handoff (a stale commit). It shows the latest handoff.
   Check: a ticket reaching Review a second time shows its second handoff.
20. Delivery problems read right — waits for nothing
   RESULTS-C (canvas ticket-05b). A failed merge check's sheet badge says "Merge conflict · fixing" while the card
   says "Merge check failed · fixing" (crates/otaq-core/src/domain/validation.rs:23): name the merge check. The
   conflict banner on Overview says what moved ("SHOP-6 changed README.md"), that nothing was merged and it returns to
   Review when fixed, and the count ("1 of 3"), as drawn.
   Check: a failed merge check and a conflict each show their own words on the badge and Overview.
21. The send-back reason is the agent's — waits for nothing
   RESULTS-C (UX-55). The agent's reason was posted as the user's comment ("You · Dev asked to send NOTE-2 back…").
   It is the agent's: History records the agent's reason and the user's answer; the stage sent back to reads it, by
   the agent's name.
   Check: after an agent's send-back request and the user's Send back, the reason is shown as the agent's.
22. Noyan's panel lists the changes it made — waits for nothing
   RESULTS-B (canvas noyan-01 · 4, brief 2b). Noyan made SHOP-7 wait for SHOP-6 (`otaq dep add`) but the panel showed
   only CREATED 2, no CHANGED row. Changes Noyan makes to tickets, dependencies included, are listed under CHANGED.
   Check: after Noyan adds a dependency, the panel lists it under CHANGED.

<!-- tuq:plan:end -->
