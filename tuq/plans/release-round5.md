# Plan · release-round5

- **Ticket:** none
- **Date:** 2026-11-11
- **Decisions:** 040, 041, 042, 043
- **Tech spec:** none (bug fixes and the decisions above; each issue's evidence is in ideas/ux-rethink/build/release-test/issues-round5/, the re-test in ../RESULTS-retest-5.md)
- **Orchestrator prompt:** tuq/prompts/release-round5-orchestrator.md

<!-- tuq:plan:start -->

Round 5 fixes what the Windows re-test of release-fixes 528d738a (re-test 5) and the round-4 piece agents left open,
logged one per file in ideas/ux-rethink/build/release-test/issues-round5/ (NN-….md; screenshots in
../shots-retest-5/, the re-test in ../RESULTS-retest-5.md). It is built after the git-hooks milestone
(tuq/plans/git-hooks.md) is merged into release-fixes, on that branch. Ten pieces, one behaviour each, following
decisions 040–043. Most run side by side. The ones that change how a session's prompt reads (the fixing state, the
focused terminal) wait for piece 2, which changes how Otaq keeps a session's prompts, and the "Unsent text" footer
waits for piece 1, which changes how Otaq knows a line is empty. The agent in charge follows
tuq/prompts/release-round5-orchestrator.md.

## Pieces

1. Otaq types into an agent's line only when the screen shows it empty — waits for nothing
   Issue 01 (must fix; re-test 5's FAIL of round-4 piece 26). Unsent text pasted into a Codex session (Ctrl+V in the
   app's terminal) held the restack request only until the first Backspace: Otaq then typed `$otaq:restack …` after
   the rest of the text and Codex got both as one message (`c26-FAIL-restack-typed-after-one-backspace.jpg`,
   `c26-FAIL-sent-as-one-message.jpg`). Likely the unsent-input tracking counts a paste (or bracketed paste) as less
   than its length, so one delete reads as empty and the screen check is skipped. Every automatic message (deliveries,
   the restack, the stage reminder, writer requests) goes into a line only when the screen shows it empty, whatever
   the keystroke count says. crates/otaq-core/src/session/ (`has_unsent_input` and the input tracker),
   session/deliveries.rs, service/session_requests.rs. Round-4 piece 26's test
   (crates/otaq-core/tests/suite/merge_check.rs, `the_restack_request_waits_behind_unsent_text_…`) types byte by byte;
   add the paste case.
   Check: a test pastes a line as one chunk, sends one delete, fails the merge check: the restack request waits; after
   the line is emptied it is typed alone.
2. Every open prompt of a session reads Needs you, one at a time — waits for nothing
   Decision 040, issue 02. Codex fires several PermissionRequest hooks at once when it runs commands in parallel but
   draws one prompt at a time; Otaq keeps one open prompt per session, so after "y" on the first the next never reads
   Needs you (a checker stayed "Checking" 30 s+, 3 of 3) and the card may name a command not yet shown
   (`c28-checker-2nd-prompt-30s-still-checking.jpg`, `c28-note16-checker-card-names-other-command.jpg`). Keep every
   open prompt of a session; Needs you names the one on screen (read the screen to tell which); after it is answered
   the next one reads Needs you at once. For every agent session kind: a ticket's agent and delivery session
   (crates/otaq-core/src/signals/asks.rs, mod.rs), a checker (validation/mod.rs), a helper (helpers/waits.rs), Noyan
   (project_agents/mod.rs). Codex's hooks: agent_tools/codex/hooks.rs. Keep round-4 pieces 23 and 28 working (a key
   alone answers; a prompt tells the app at once).
   Check: tests where two prompts open together for a checker, a helper and a ticket's agent: the first reads Needs you
   naming the first command; "y" moves Needs you to the second, naming it; "y" again clears it.
3. A prompt while a merge check is being fixed reads Needs you — waits for 2
   Issue 03 (must fix). SHOP-32's Dev agent stopped at Codex prompts (`git rebase main`, `git diff …`) while the ticket
   read "Merge check failed · fixing"; the pill and card kept that and the strip had no Needs you
   (`new-fixing-prompt-pill-no-needs-you.jpg`, `new-fixing-prompt-card-no-needs-you.jpg`). A prompt or question in any
   "fixing" state (merge check failed, conflict) reads Needs you like anywhere else, and the fixing state comes back
   once it's answered. The signals' transition (crates/otaq-core/src/signals/mod.rs) and the fixing statuses
   (crates/otaq-core/src/domain).
   Check: a test with a ticket in "Merge check failed · fixing" whose agent opens a prompt: Needs you on the card and
   the strip; after "y" it reads "fixing" again.
4. A message for a session that ended doesn't wait forever — waits for nothing
   Issue 04, first part (must fix). After a send back, SHOP-32's Dev session header read "1 message waiting · Deliver
   now" until the merge: the waiting row in `pending_deliveries` belonged to the ended round-1 checker session ("The
   user added comment 2. Run `otaq prime`…") and never went (`new-unsent-text-claimed-line-empty.jpg`). A message
   queued for a session that has ended goes to the ticket's live agent session when it is meant for the ticket's
   agent, and is dropped otherwise; no session's header counts another session's messages.
   crates/otaq-core/src/session/deliveries.rs and the session header's waiting count (view model).
   Check: a test queues a message for a checker session, ends it and sends the ticket back: the Dev session's header
   shows no waiting message that will never go.
5. "Unsent text in the input line" shows only when the line holds text — waits for 1
   Issue 04, second part. The footer "Unsent text in the input line · Otaq's note waits until it's empty" showed over
   Codex's empty line (perhaps after a "y" that a closed prompt didn't take). The footer follows the same "is the line
   empty" answer as piece 1: a key a tool's prompt or menu took, or one typed after a prompt closed and then cleared,
   never leaves the footer up over an empty line.
   Check: a test where "y" goes to a prompt that has just closed and the line ends empty: no footer, and a waiting
   message is typed.
6. The card says Needs you while you're in the agent's terminal — waits for 2
   Decision 041, issue 06. With the session's terminal focused ("Typing goes to the agent · you're here, so no
   alert"), a Codex prompt left the card "Working" while the sheet's pill read "Needs you", 12 s+
   (`c29-focused-card-working-pill-needs-you.jpg`). Focus holds back that agent's alerts only (sound, notification,
   strip ping); the card, pill and session state always show the real state. crates/otaq-core/src/attention/focus.rs
   and what it suppresses; the card's view model.
   Check: a test with the terminal focused and a prompt open: no alert, and the card reads Needs you like the pill.
7. A "login expired" stop offers Sign in… at once — waits for nothing
   Issue 05. NOTE-16 stopped "login expired"; its sheet's callout read "Sign in to Claude Code, then Resume" with only
   Resume and Answer in terminal; Sign in… appeared only after opening Settings → Agents, which re-checks the tools
   (`c25-sheet-no-sign-in-before-recheck.jpg`, `c25-sheet-sign-in-button.jpg`). A login-expired stop marks the tool
   signed out (or re-checks it) at once, so the callout, the agent panel's notice and the strip offer Sign in…
   straight away. crates/otaq-core/src/agent_tools/connected.rs and the stop reason; the tool-problem callouts.
   Check: a test where an agent stops with a login-expired error: the tool reads signed out and the ticket's callout
   has Sign in… without a tools check.
8. On Windows, Ctrl+C in a terminal copies when text is selected — waits for nothing
   Decision 042, issue 07. In Otaq's terminals on Windows, Ctrl+C with a selection copies it and clears the selection,
   and the program gets nothing (Codex clears its input line on Ctrl+C, so a copy habit lost drafts); with no
   selection it goes to the program as today. Ctrl+Shift+C keeps copying; macOS and Linux unchanged. The terminal's
   key handling in crates/otaq-desktop/ui/src (the terminal component and its copy shortcut).
   Check: a vitest on Windows key handling: Ctrl+C with a selection copies and sends nothing; without one it sends the
   interrupt; on macOS nothing changes.
9. Every view names the stage a held ticket starts at — waits for nothing
   Issue 08. Round-4 piece 27 put a held ticket (or one waiting for an agent, or whose start stopped) in the column of
   the stage it starts at, on the board and List view (`Columns::with_starts`, `board::columns(input)` in
   crates/otaq-view/src/view_models/board.rs and flow_columns.rs). Views built from plain `Columns::of` still name the
   flow's first agent stage for such a ticket: the command palette, the sheet's column id, dependencies, the filter.
   Use the same start stage there. A Queued ticket stays at the top of the first stage, as piece 27 kept.
   Check: view tests on a Spec first flow with a held ticket that starts at Dev: the palette, the sheet, dependencies
   and the filter all name Dev.
10. A newer "Ask the agent to write it" replaces one that never started — waits for nothing
   Decision 043, issue 09. Round-4 piece 24 refuses a write-skill for another stage while the same agent was asked
   for a stage and hasn't named a folder yet (crates/otaq-core/src/flow/write_skill.rs `asked_for_another`), so a
   dropped request blocked every new one. A newer request for a different stage drops an older one that named no
   folder (its stage stops showing "Being written…"); one that already named a folder is left alone. The agent may
   still only write the stage it was asked for.
   Check: a test asks for stage A's skill, then (no folder named) for stage B's: B's request goes through, A no longer
   reads "Being written…"; with A's folder named, B is refused as today.

<!-- tuq:plan:end -->
