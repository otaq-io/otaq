# Plan · release-issues

- **Ticket:** none
- **Date:** 2026-11-10
- **Decisions:** 011, 012, 013, 015, 016, 017 (014, 018 and 019 keep what is built: no piece)
- **Tech spec:** none (bug fixes and the decisions above; each issue's evidence is in ideas/ux-rethink/build/release-test/issues/)
- **Orchestrator prompt:** tuq/prompts/release-issues-orchestrator.md

<!-- tuq:plan:start -->

The Windows re-test of release-fixes (7f36bd79) and the piece agents left 19 issues, filed one per file in
ideas/ux-rethink/build/release-test/issues/ (NN-….md, named per piece below; the screenshots they cite are in
../shots-retest/, the re-test in ../RESULTS-retest.md). All of them are fixed before the release, in 22 pieces of one
behaviour each: the two re-test failures (01, 02), the bugs (04–10, 15–18), and the product decisions 011–017 taken
while reading them. Decisions 014, 018 and 019 keep what is built and need no piece. Issue 03 is a re-test, not code:
it is in the orchestrator prompt's final re-test. Nearly everything runs side by side; only the restart dialog's
Sign in… waits for its tool state to be right, and Noyan's Undo and its CREATED-only row wait for the what-changed
text. The agent in charge follows tuq/prompts/release-issues-orchestrator.md.

## Pieces

1. The restart dialog reads each tool's real state — waits for nothing
   Issue 01 (must fix). Right after a restart, the "agents were stopped by a restart" dialog called an installed,
   signed-in Codex (and, in another run, Copilot) "isn't available (not installed)", left the row unticked and said
   "Resume 0", while Settings → Agents in the same session said Connected. Find where the dialog gets the tool's state
   (it may read it before the service's first tool check has finished, or from the wrong tool):
   crates/otaq-core/src/agent_tools/connected.rs (NotInstalled), crates/otaq-view/src/view_models/after_restart.rs,
   crates/otaq-desktop/ui/src/screens/recovery/AfterRestart.tsx. A row must never say "not installed" for a tool
   that is; while a tool's check hasn't finished, the row waits for it and then updates.
   Check: a test restarts with a Codex agent and a Copilot agent whose tools are installed and signed in, with the
   tool check still running when the dialog is built: both rows end up ticked and the button says "Resume 2".
2. The restart dialog's title counts every stopped agent — waits for nothing
   Decision 011. The title counts all stopped agents ("3 agents were stopped by a restart"), whether or not they can
   resume; the Resume button counts the ticked rows only ("Resume 2") and follows ticking. This changes what piece 4
   of release-fixes built (the title counted only the agents that could resume).
   crates/otaq-view/src/view_models/after_restart.rs, ui/src/screens/recovery/AfterRestart.tsx.
   Check: a restart with two resumable agents and one on a signed-out tool shows "3 agents were stopped…" and
   "Resume 2"; unticking one makes the button "Resume 1" and leaves the title.
3. Sign in… on a signed-out row of the restart dialog — waits for 1
   Decision 012. A row unticked because its tool is signed out shows its reason and Sign in…, which starts the same
   sign-in as Settings → Agents. When it succeeds, the row becomes ticked and the button counts it ("Resume 2" →
   "Resume 3"); the title doesn't change. If it fails or is dropped, the row stays unticked with its reason and can be
   tried again. Rows that can't resume for any other reason get no Sign in…. One behaviour across the dialog's view
   model, its page and the sign-in call, on purpose (after_restart.rs, AfterRestart.tsx, the sign-in Settings →
   Agents uses).
   Check: with a signed-out tool's row, Sign in… starts sign-in; after a successful sign-in the row is ticked and the
   button goes up by one; a failed one leaves it unticked.
4. A stage whose Codex skill has a byte order mark warns — waits for nothing
   Issue 02 (must fix). A personal Codex skill whose SKILL.md starts with a byte order mark (EF BB BF): Pick a skill…
   says "Codex doesn't load it: its SKILL.md starts with a byte order mark" and a ticket reaching the stage is held,
   but the stage itself (its card in Edit flow and its panel) shows no warning, also after Cancel and reopening. The
   stage warns as it does for a missing skill. Start at crates/otaq-core/src/flow/proposal.rs ~997 (the message),
   agent_tools/codex/mod.rs ~116, and where the stage's skill warning is worked out
   (crates/otaq-view/src/view_models/flow_editor.rs, flow_board.rs, tool_problems.rs).
   Check: with a BOM SKILL.md as the stage's Codex skill, the stage's card and panel show the warning; without the
   mark, none.
5. A queued writer request is dropped when the ticket leaves Review — waits for nothing
   Issue 04. When the delivery's agent is the ticket's own Dev session and the merge check sends the ticket back to
   Dev, the queued "You are its delivery's agent: write the message…" request (and the restack request after it) was
   still typed into the Dev session once its input line was free. A queued writer request is dropped when the ticket
   leaves Review / To deliver / PR open. Look at how session requests queue behind unsent text
   (crates/otaq-core/src/service/session_requests.rs, crates/otaq-core/src/session/) and the writer request.
   Check: a test queues a writer request behind unsent text, sends the ticket back to Dev, frees the line: nothing is
   typed in.
6. The Approve dialog never says someone is writing when no one is — waits for nothing
   Issue 05. SHOP-10's Approve dialog said "Developer is still writing the message…" forever: the writer had been
   asked once under the old build, that session was gone, and it was never asked again because the branch head didn't
   change. This can happen to anyone who updates the app with a ticket in Review. When no writer session is running
   and no message was written, the writer is asked again; if it can't be, the dialog offers the plain message. Decision
   010 holds for a writer that is running. crates/otaq-view/src/view_models/pr_dialog.rs, review_page.rs, pr_page.rs and
   where the writer is asked.
   Check: a ticket in Review with a recorded writer request and no writer session: opening Approve asks again (or
   offers the plain message), never "still writing" forever.
7. The running chip shows after Keep them running — waits for nothing
   Issue 06. Tray → Quit Otaq → Keep them running, then reopen: the title bar showed Pause but no "1 running" chip,
   while the tray said "1 agent running" and the card said "Working · in the background". The chip counts what the
   tray counts (piece 14 of release-fixes made the tray and Quit agree).
   crates/otaq-view/src/view_models/quit.rs, status_icon.rs, crates/otaq-desktop/src/door.rs, the title bar's chip.
   Check: after Keep them running and reopening, the title bar shows "1 running", matching the tray.
8. An agent idle at its prompt doesn't read as Working — waits for nothing
   Issue 07. SHOP-13's Codex agent answered and sat idle at its prompt without calling `otaq stage next`; the card
   stayed "Working · in the background" and Session said "Waiting on its background work", so the user was never
   asked to act. When the agent's tool is idle at its prompt and nothing of its own is running in the background, the
   ticket reads idle and needs the user, as any agent that stops without finishing its stage.
   crates/otaq-view/src/view_models/card.rs, card_line.rs, crates/otaq-desktop/src/terminal.rs, the session state.
   Check: a Codex session idle at its prompt with no background work shows the idle / Needs you state, not Working.
9. The merge-check failure dialog shows the check's output — waits for nothing
   Issue 08. "Merge check failed on main" shows an empty pink box; the output is only in the attached log
   (SHOP-14.log). The box shows the check's output (its end, when long), with the log still attached.
   crates/otaq-view/src/view_models/approve.rs, review_page.rs, ui/src/screens/sheet/.
   Check: a failed merge check's dialog shows the failing output in the box.
10. Cancel in Edit flow removes the skill the agent wrote — waits for nothing
   Issue 09, first half. "Ask the agent to write it…" wrote .agents/skills/update-docs/SKILL.md into the project;
   after Cancel in Edit flow the draft went away but the file stayed, untracked. Cancel removes what the agent wrote
   for that draft (only files it wrote, never the user's own). crates/otaq-core/src/flow/write_skill.rs, draft.rs,
   crates/otaq-desktop/src/api/flow_editor.rs.
   Check: write a skill with the agent, Cancel: the file is gone; a skill the user had before stays.
11. A written but uncommitted skill's warning says so — waits for nothing
   Issue 09, second half, agreed with the maintainer on 2026-11-10: after the agent writes the missing skill
   ("Written by the agent · Open · Undo"), the stage's card no longer says "You don't have the skill update-docs";
   it warns that the skill is written but not committed yet, and says what keeps it (for example "update-docs is
   written but not committed yet"; use the words that match what Apply does to the file).
   crates/otaq-view/src/view_models/flow_editor.rs, flow_board.rs, ui/src/screens/flow_editor/.
   Check: after the agent writes the skill, the stage's warning says it is written and not committed, not that it's
   missing.
12. Edit flow's draft line names stages by name — waits for nothing
   Issue 10. Removing a stage with a held ticket says "(1 in update-the-docs)"; it names the stage as the user sees it,
   "(1 in Update the docs)". Same for every stage id in that line (RESULTS-D saw "validate").
   crates/otaq-view/src/view_models/flow_board.rs, flow_editor.rs.
   Check: removing a stage with a held ticket shows the stage's name in the draft line.
13. Noyan's CHANGED row says what changed — waits for nothing
   Decision 013, issue 14. The row showed only "✎ SHOP-16 Retest dep B · Backlog". It says what Noyan changed, as
   drawn on canvas noyan-01 · 4: "SHOP-7 · now waits for SHOP-6", and in plain words for each kind of change Noyan
   can make to a ticket. Its agent in release-fixes said this needs a change to what the service tells the panel about
   a change. One behaviour across the service's record of the change and the panel, on purpose
   (crates/otaq-view/src/view_models/agent_panel.rs and the Noyan change records in otaq-core).
   Check: after Noyan makes a ticket wait for another, its CHANGED row reads "<ticket> · now waits for <other>".
14. Undo on Noyan's CHANGED row — waits for 13
   Decision 013. Each CHANGED row has Undo, which takes that change back on the ticket. Undo shows only while that part
   of the ticket is still as Noyan left it: once the user or an agent changed it again, the row keeps its text and
   loses Undo, so Undo never overwrites someone's later edit. A ticket that only moved to another column keeps Undo.
   Check: Undo on "now waits for SHOP-6" removes the wait; after the user edits that wait themselves, Undo is gone;
   after a column move, it is still there.
15. A ticket Noyan created and changed shows under CREATED only — waits for 13
   Decision 013. A ticket Noyan created and then changed in the same turn shows once, under CREATED, with what it was
   given ("SHOP-16 · waits for SHOP-15"), and gets no CHANGED row. Today it shows under both.
   Check: Noyan creates a ticket and makes it wait for another in one turn: one CREATED row with the wait, no CHANGED
   row.
16. A skill is named the way the stage's tool calls it — waits for nothing
   Decision 015, issue 12. "Codex doesn't have the skill `/retest-docs`" should say `$retest-docs`: wherever Otaq names
   a skill in a message about a stage (the held message, stage warnings, Pick a skill…'s reasons), it uses the call
   form of that stage's tool: `$name` for Codex, `/name` for Claude Code, each other tool's own form.
   crates/otaq-view/src/view_models/agent_reads.rs, flow_editor.rs, flow_board.rs, card_page.rs, tool_problems.rs.
   Check: a Codex stage's held message says `$retest-docs`; a Claude Code stage's says `/retest-docs`.
17. "Drafting a flow" gives way to Needs you while the agent waits — waits for nothing
   Decision 016. While the flow-drafting agent is stopped at a permission prompt or a question for the user, Needs you
   replaces the "drafting a flow" line; once the user answers and the agent works again, the line comes back.
   crates/otaq-view/src/view_models/flow_board.rs, ui generated FlowBoardDrafting.
   Check: a drafting agent at a permission prompt shows Needs you, not "drafting a flow"; after the answer, the line
   is back.
18. A send-back comment names each stage once — waits for nothing
   Decision 017. The comment reads "<stage> sent it back to <earlier stage>: <reason>" ("Dev sent it back to Product
   spec: …"), not "Dev asked to send … back to Product spec: …". It stays the agent's comment (release-fixes piece 21).
   crates/otaq-core/src/service/board_requests.rs, crates/otaq-view/src/view_models/history.rs.
   Check: after a send-back the comment reads "Dev sent it back to Product spec: <reason>".
19. Codex's Needs you clears once its prompt is answered — waits for nothing
   Issue 15, not yet reproduced. After a Codex permission prompt is approved in the terminal, the ticket kept "Needs
   you" for a while before returning to Working. Reproduce first (Codex on Windows in the VM, where piece 14's agent saw
   it); then make the state follow the prompt going away. If it can't be reproduced, say so and change nothing.
   Check: approving a Codex permission prompt returns the ticket to Working at once (within the session's usual
   screen check).
20. Clicking a Windows notification opens Otaq on its ticket — waits for nothing
   Issue 16. Toasts are titled Otaq with its icon (release-fixes piece 13), but clicking one does nothing. A click
   brings up Otaq's window, on the ticket the notification is about when it has one. crates/otaq-desktop/ (the
   notification code piece 13 changed).
   Check: on the NSIS-installed app in the Windows VM, clicking a ticket's toast brings up Otaq on that ticket.
21. A parked ticket names the stage it was last parked from — waits for nothing
   Issue 17, suspected only. Release-fixes piece 19 fixed "What the agent did" showing an older round; its agent saw
   the same newest-first pattern in parked_from (crates/otaq-desktop/src/api/board.rs:437). Write a test with a ticket
   parked from two different stages in two rounds; fix it if it names the older one, otherwise change nothing and say
   so.
   Check: the test shows a ticket parked twice names the stage of its latest park.
22. The flaky tests wait on state, not time — waits for nothing
   Issue 18. worktree_skills::an_agent_stage_whose_tool_lacks_its_skill_is_held_until_check_again
   (crates/otaq-core/tests/suite/worktree_skills.rs ~716, the check_again after a git push) failed once under the full
   `cargo nextest run --workspace` and passed alone; client_start::restart_replaces_the_service_process in otaq-cli did
   the same. Make each wait for the state it checks instead of a time.
   Check: both pass in five full `scripts/check.sh` runs in a row.

23. A key alone answers a Codex prompt in every agent session — waits for the round-2 fixes of 7, 8 and 17
   Found by piece 17's second round (the Windows re-test of release-issues, RESULTS-retest-2.md). Codex answers its
   permission prompt with "y" alone, no Enter (`AgentTool::key_alone_approves_prompt`). Piece 19 made that key clear
   the prompt for a ticket's Agent and Delivery sessions (`Signals::key_answered_prompt`), and piece 17's round 2 for
   Project agents (`ProjectAgents::key`); a helper's wait still clears only on Enter or a signal
   (crates/otaq-core/src/helpers/waits.rs `enter`), and validators and Answer sessions may have the same gap. All of
   them are hooked up in crates/otaq-core/src/service/session_requests.rs. Find every agent session kind that can stop
   at a permission prompt and make a key alone answer it there too, on a tool that takes one, the same way for each
   kind (one shared path rather than a branch per kind, if the code allows). Escape sequences and pastes don't count.
   Check: for each such kind, a test with a Codex session at a prompt gets "y" and is no longer waiting (the helper's
   Needs you / the ticket's state goes back to working), and a Claude Code session still needs Enter.

## Round 4 (after re-test 4, base 5e8e9fc7)

24. A skill asked for on a new stage goes to that stage — waits for nothing
   RESULTS-retest-2.md NEW-1: "Ask the agent to write it…" on a new stage "Update the docs 2" wrote the skill onto
   the other stage "Update the docs" (Noyan ran `otaq flow write-skill update-the-docs --done`); the new stage stayed
   "Being written by Noyan…". Two stages with near names; the request names the stage by name only.
   Check: a test with stages "Update the docs" and "Update the docs 2": asking for the second's skill fills the second
   and leaves the first's skill as it was.
25. Sign in… on a ticket's sheet shows its terminal — waits for nothing
   RESULTS-retest-2.md NEW-2: Sign in… on a ticket's sheet (Claude Code stopped · login expired) starts
   `claude auth login`, but the terminal panel that opens shows "Nothing open". The login's terminal shows where the
   user can see it, as the restart dialog and Settings → Agents show theirs.
   Check: a test (UI and/or view model) that Sign in… on the sheet opens a panel showing the sign-in's terminal.
26. A request Otaq types never wipes unsent text — waits for nothing
   RESULTS-retest-2.md NEW-3: after a failed merge check, the restack request was typed into SHOP-19's Dev Codex
   session and the user's unsent text there was gone. Every automatic message waits behind unsent text, as
   deliveries do (the previous re-test saw the restack wait).
   Check: a test with unsent text in the agent's input line: the restack request waits, and is typed once the line is
   empty.
27. A ticket's stage names are its own — waits for nothing
   Three small wrong-stage displays: the Park dialog says "resumes on Dev" for a ticket parked from another stage (it
   resumes there) (RESULTS-retest-2 and -3 NOTEs); a ticket held for a signed-out tool sat in the Product spec column,
   a stage it skips (RESULTS-retest-2 NOTE); a closed ticket's stage dots tick stages it never ran (RESULTS-retest-3
   NOTE, NOTE-15 closed from Product spec ticked Tech spec, Milestone plan and Dev).
   Check: tests for each: the dialog names the stage it was parked from; a held ticket stays in a column it runs; a
   closed ticket ticks only stages it passed.
28. A Codex prompt on screen reads Needs you at once — waits for nothing
   RESULTS-retest-3.md NEW-2: a Codex prompt drawn on screen often reaches Needs you late: helpers 4–14 s, a checker
   ~30 s, Dev prompts read Working. Find what Otaq waits for (Codex's PermissionRequest hook vs. the screen) and make
   the state follow the prompt within the session's usual screen check.
   Check: a test where Codex's prompt shows (as its hook or screen reports it) and the ticket/helper/checker reads Needs
   you at once.
29. An idle agent in charge and the card read their real state — waits for nothing
   RESULTS-retest-3.md NOTEs: NOTE-14's Developer in charge, idle at its prompt after Dev ended, was listed "needs you"
   during Validate and "working" in Code review; a card behind an open sheet lagged its pill for seconds (Working vs.
   Idle / Needs you; also RESULTS-retest-2 NOTE, ~20 s).
   Check: tests that an idle agent in charge reads idle in every later stage, and that the card and the sheet's pill
   read the same state from the same update.
30. Small dialog and panel fixes — waits for nothing
   The restart dialog closes after Resume (RESULTS-retest-3: it stayed open showing "Resumed" and a greyed "Resume 0"
   until Not now); the strip chip's tooltip caps a long prompt at 600 characters like Edit flow's; decision 022 (Edit
   flow's remove note only with tickets in the stage).
   Check: tests for each.
31. The Git check skips the agent tools, and uninstall removes the otaq:// link — waits for nothing
   Settings → Git & pull requests' check reruns the agent-tools check before git, gh and az, which it doesn't need: it
   checks only those. Decision 024: uninstalling Otaq removes the `HKCU\Software\Classes\otaq` key piece 20's
   installer added (crates/otaq-desktop hooks.nsh); check it does, and add it if not.
   Check: a test that settings.git doesn't run the agent-tools check; the uninstaller section removes the key (and,
   in the Windows re-test, the key is gone after uninstall).
32. Noyan's later-turn change and an undone row — waits for nothing
   Decision 021: a change in a later turn to a ticket Noyan created earlier gets its own CHANGED row with Undo; the
   earlier CREATED row keeps its title (RESULTS-retest-2 NEW-4). Decision 023: after Undo the row says "Undone · …".
   Check: tests for both.

33. Sending a ticket back closes its open question — waits for nothing
   Decision 025. Today a ticket's open question for the user (human_questions, answered_at NULL) is closed only by an
   answer, a newer question from the same agent, or the agent's own send-back question
   (crates/otaq-core/src/board/questions.rs, service/board_requests.rs ~1986); AnsweredVia::Closed ("its ticket was
   closed, parked or delivered") is applied only to helpers. A send back (human check's Fail, drag back, accepted
   send-back request) leaves the question open, and signals treat an open question as holding Asks you
   (signals/mod.rs "The store decides").
   Check: tests that after each kind of send back, and after close, park and delivery, the ticket has no open question,
   leaves Asks you, and the earlier stage's turn end doesn't bring Asks you back.

<!-- tuq:plan:end -->
