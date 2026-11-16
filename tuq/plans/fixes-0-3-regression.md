# Plan · fixes-0-3-regression

- **Ticket:** none
- **Date:** 2026-11-16
- **Decisions:** 113, 114, 116, 117, 118; refined 081, 108, 110 (115 is after 0.3)
- **Tech spec:** none (findings of the 0.3 Windows regression test, investigated and reviewed; see below)
- **Orchestrator prompt:** tuq/prompts/fixes-0-3-regression-orchestrator.md

<!-- tuq:plan:start -->

The 0.3 Windows regression test (main 78e23752) found 22 issues; the maintainer chose to fix them before 0.3 (all but
OpenCode's per-provider limit, decision 115, which moves after 0.3). One piece per behaviour, narrow on purpose: each
changes only what its behaviour needs and proves with tests that what must not change didn't. Evidence and analysis
live in `/Users/nevzattopcu/Desktop/Workspace/otaq/ideas/ux-rethink/build/release-test/0.3/` (not in git: read it by
that absolute path): `REPORT.md`, `RESULTS-<phase>.md` (finding ids such as F-E2-2), `shots-*/`, the investigations
`investigate-<n>-*.md`, and three independent reviews of this plan against the code, `review-plan-a.md` (pieces 1–4,
6), `review-plan-b.md` (5, 11–16 in the first draft's numbering, kept below) and `review-plan-c.md` (8, 9, 17–22).
**Each piece reads its investigation and its review first and applies the review's changes**; where this plan and the
review differ, the review's file:line findings win unless they contradict a decision, in which case the piece stops
and reports. All pieces go side by side. Piece 5 also changes the otaq-skills submodule (`plugin/skills`, on a branch
there, pointer bumped here); nothing is released.

## Pieces

1. An idle Copilot in an unfocused terminal is not a dialog — waits for nothing
   F-E2-2; investigate-1 + captures; review-plan-a piece 1. `session/input_line.rs` `read_all`: with the cursor
   hidden, a pointed numbered menu is a dialog; otherwise a drawn input row means not a dialog; otherwise a dialog as
   today. Per the review: an input row must also exclude unnumbered pickers (Copilot `/model`, Claude Code `/resume`)
   and OpenCode's button prompt — capture those screens hidden-cursor in the Windows VM and add them as fixtures;
   and fix the 500 ms loop in `deliveries.rs` `after_dialog` (stop when nothing waits) rather than only quieting its
   log. Note the review's side effect at `quiet.rs:338` in the report.
   Check: unfocused idle Copilot (both layouts) reads as no dialog; unfocused permission/folder-trust prompts,
   pickers and OpenCode's prompt read as dialogs; existing input_line and quiet_turn_ends tests unchanged; no repeating
   recheck with nothing waiting.
2. Copilot's removed "Auto · efficiency": not offered, saved agents held, resumes go on as balance — waits for nothing
   Decision 117; F-E2-1 (a); investigate-2; review-plan-a piece 2. `copilot/models.rs` expands `auto` into balance and
   intelligence only; `CopilotTier::Efficiency` stays so saved values parse. Per the review, the stored model list
   copy must drop `auto@efficiency` at once on upgrade (not only after the next successful read from Copilot), so the
   launch check holds it even offline. A resume of a conversation that ran on efficiency launches with
   `--auto-tier balance` (resumes only; new starts stay held). CHANGELOG line.
   Check: tier tests; a saved `auto@efficiency` agent held on a new start with the stale list too; a resume of an
   efficiency conversation passes `balance`.
3. A tool that exits before its conversation exists shows Start failed with its own message — waits for nothing
   F-E2-1 (b); investigate-2; review-plan-a piece 3. `recovery/mod.rs` `agent_session_ended`: after the existing "not
   found → Resume failed" check, a fresh start (not a resume) that ended on its own with no conversation goes to
   `start_failed`, for Claude Code and Copilot only (OpenCode creates its conversation before launch and its
   `can_resume` runs a slow command; Codex is covered already). Never reclassify a resume (REC-03). The message adds
   the tool's last 1–3 meaningful screen lines (border-only rows stripped). Keep the stage as `Starter::model_refused`
   does.
   Check: no conversation on a fresh start → Start failed with the tool's lines; an existing conversation stays
   Interrupted; a resume stays Resume failed or Interrupted as today; model changed then Retry start uses it.
4. An answer session waits only for its own tool's limit — waits for nothing
   F-F-1; investigate-3; review-plan-a piece 4. `messages/answering.rs` `route_waiting` and `resume_target` check the
   limit of the tool of the target's stored conversation, read from history only (never `can_resume`, which runs
   `opencode`). Keep today's rule that a limit holds until the user's Resume for that tool (d0020: nothing resumes by
   itself); only the "any tool" part changes. `resume_target` refuses only in the no-live-session branch and names
   the tool. Leave `start_answer_session`'s `unavailable(None)` as a noted follow-up.
   Check: another tool's limit doesn't hold a Claude Code target; the target's own limit still holds and the existing
   test passes unchanged.
5. A check stage runs otaq:check-ticket, then its task skill — waits for nothing
   Decision 113 (refined: a checker doesn't ask the user), 081; F-U2-2; review-plan-b piece 5 (its real file list
   replaces this plan's). Skill: `check-ticket` in the otaq-skills submodule, like `work-ticket` for a checker
   (`otaq prime --validate`, read-only, ends with `otaq check pass|fail --stdin`, open points into a fail or a
   send-back, how it meets the task skill per 098), added to `shipped::SKILLS`/`SHIPPED_TEXTS` and the prefixed copy;
   description within Copilot's 1024. Code: `check_prompt` starts `otaq:check-ticket` when the stage's skill isn't
   `otaq:validate`; validator sessions get turn tracking; at a checker's turn end a waiting task skill is typed before
   the no-result rule applies (order matters); `otaq check` is refused while the task skill waits, like `stage next`
   (`board_requests.rs`); the waiting skill is stored before the session spawns (start race). `otaq:validate` stages
   and work stages unchanged. Hide `otaq:check-ticket` from Pick a skill… (it's a workflow skill, not a stage's).
   Check: a stand-in user check skill: `/otaq:check-ticket` → task skill typed → verdict lands; an early `otaq check`
   is refused; an `otaq:validate` stage and a work stage start exactly as before.
6. The Running popover lists what holds each slot — waits for nothing
   Decision 114; F-B-2; investigate-5; review-plan-a piece 6. Send every slot holder (`slot_holders()`, `#[serde(default)]`
   on the new status field) and let the view list, with the limit on, the holders that aren't already among the
   working agents ("<KEY> · waits for you · <what>", or starting / held), so the count and the list always agree;
   "Nothing running" only when both are empty; Settings "Right now: N of M slots taken"; "Run at most" says a waiting
   agent keeps its slot. `HOLDS_A_SLOT`, counts, free slots and the queue unchanged; regenerate ts-rs types and the
   protocol snapshot.
   Check: core/view tests per investigate-5 plus the review's three cases (start in progress, limit wait with no
   session, slot holder whose session is gone); limit off unchanged.
7. A check judges only what changed since the user last approved — waits for nothing
   Decision 116; F-E1-3; `investigate-116-checks-since-approval.md` (its approach, size and risks decide this piece's
   shape; if it says L, stop and report before building). Approval steps (Spec review, Review, any user stage) are one
   kind; a later check's scope starts at the last approval; earlier approved changes stay visible as context; no
   stage or skill special-cased.
   Check: on a Spec first flow, a change approved at Spec review isn't failed by Validate after Dev; with no approval
   the whole branch is judged as today.
8. Agents' summaries and verdicts are drawn as Markdown — waits for nothing
   Decision 110 refined; F-C-1; review-plan-c piece 8. `CommentMarkdown` for "What the agent did", the checker
   verdict and the Agent summary (keep its 12px secondary look); the structured Validation report keeps its
   structure and draws inline Markdown per line, ticket keys still linked (also inside backticks).
   Check: page tests for each place, the report's findings/notes/Create ticket still there.
9. Pick a skill… is wide enough to read — waits for nothing
   Decision 108 refined; F-D-1, F-D-2; review-plan-c piece 9. `width={360}` and the two-line cut with hover on this
   picker only; `shipped_caption` gets a step-back line.
   Check: page tests; step-back has a caption with no tuq installed; other menus unchanged.
10. Codex at its own sign-in screen asks the user to sign in there — waits for nothing
   Decision 118; F-E2-4; review-plan-b piece 14. First capture Codex's sign-in screen in the Windows VM (the VM's
   Codex sign-in is expired; use a scratch CODEX_HOME with that stale auth.json, never touch `~\.codex`). Recognise it
   on the session's screen: Needs you "Codex needs you to sign in", session kept, the agent goes on after sign-in;
   Settings stops saying signed in until a Codex session gets past the screen.
   Check: a test with the captured screen; signed-in detection tests unchanged.
11. Reply on a question after a restart shows the question — waits for nothing
   F-U2-1; review-plan-b piece 11. `api/sheet.rs` `answer_of` also for an Interrupted ticket that kept its question;
   keep the action bar (Resume, callouts) with the answer box (`NeedsYou.tsx`); button "Answer & resume"; not in a
   paused project.
   Check: sheet API and page tests.
12. A tuq hold says what went wrong — waits for nothing
   F-E1-1; review-plan-b piece 12. `tool_problems.rs` `missing_skill_hint` branches on tuq skills: installer failed vs
   no tuq release has the skill; the installer's reason as a History note; user-skill advice unchanged.
   Check: tests for both tuq cases and the user-skill text.
13. otaq stage next names the next stage — waits for nothing
   F-E1-2; review-plan-b piece 13. `board/review.rs` waiting branch uses `stage_id()`.
   Check: a unit test.
14. The restart dialog shows after a restart from a terminal — waits for nothing
   F-D-3; review-plan-b piece 15. The service remembers whether the restart dialog was shown (a field plus a
   request), replacing the 15 s rule; an older service falls back to today's rule.
   Check: tests for a CLI-started service and for the fallback.
15. The PR dialog's branch check never pops a sign-in — waits for nothing
   F-C-2; review-plan-b piece 16. Only the four background calls (`pr/flow.rs`, `pr/verify.rs` as listed) get
   `GIT_TERMINAL_PROMPT=0`, `GCM_INTERACTIVE=never` and an empty `GIT_ASKPASS`; a check that can't reach the remote
   reads "Unchecked" and still allows Push & open PR; the user's own push keeps its credential prompts.
   Check: the variables on those calls only; Unchecked allows confirm.
16. The new-work box hides while a ticket is open — waits for nothing
   F-A-1/F-B-1; review-plan-c piece 17. Agreed 2026-11-16: hidden while any ticket sheet is open (refines #21's
   "beside the sheet"); Ctrl+N opens the full form then.
   Check: a page test.
17. Noyan's created row for a waiting ticket shows its title — waits for nothing
   F-B-3; review-plan-c piece 18. Created rows: key, title, then a muted "· waits for …"; changed rows unchanged.
   Check: a page test.
18. Esc in a Settings picker closes only the picker — waits for nothing
   F-B-4; review-plan-c piece 19. `ChoiceList` registers an Esc layer and retries focus, as `Menu` does.
   Check: page tests in Settings rows, the model picker and the flow editor's model fix.
19. Answer in terminal gets the keyboard; Esc still closes it — waits for nothing
   F-F-2; review-plan-c piece 20. Agreed 2026-11-16: the terminal opens focused (`focusWhenLive`), and Esc closes the
   overlay, never reaching the agent (decision 044).
   Check: page tests for focus and Esc.
20. The PR dialog's commit message keeps its line breaks — waits for nothing
   F-C-3; review-plan-c piece 21. A growing mono text area; an edited message keeps `\n\nCo-Authored-By:`.
   Check: page and API tests.
21. Installing over 0.2 leaves no old skill folders — waits for nothing
   F-U2-3; review-plan-c piece 22. After install, remove only the known 0.1/0.2 folders under
   `$INSTDIR\plugin\skills\` (guards as the review says); nothing outside it.
   Check: in the Windows VM: 0.2.0 then the new build (GUI, `/S`, `/UPDATE`), a fresh install, an uninstall.

<!-- tuq:plan:end -->
