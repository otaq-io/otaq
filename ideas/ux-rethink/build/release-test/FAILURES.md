# Release test failures, sorted (Windows VM, ux-rethink-fixes ef84b175)

27 FAIL lines across RESULTS-A…D; after checking each against the decisions, 2 aren't failures and 25 remain.

## Not failures (already decided)
- B · Noyan's "PROPOSED · 2 TICKETS" card is missing: settled 2026-11-09, Noyan writes straight to Backlog (UX-22).
- C · "Merge into another ticket…" isn't in ⋯: it stays in the Approve menu (UX-13; gap analysis).

## Blockers (a user loses work, gets stuck, or is misled)
1. D · Edit flow goes blank on shop, also after restarts, until the flow is replaced by a preset (A1). Began after a refused drag of Validate before Dev.
2. A · Claude Code shown "signed in" when it is logged out; the first ticket then fails at once (UX-16).
3. C · The prepared merge message never arrives (5 of 5); its Codex writer session's permission prompts are shown nowhere, it outlives the merge and leaves the worktree behind (UX-56).
4. C · "Tell <agent> now" never reaches a Codex agent: Codex's placeholder is read as unsent text (UX-58).
5. D · A personal Codex skill in %USERPROFILE%\.codex\skills isn't seen; the stage holds forever (no-skill-copies S5/S6).
6. D · Removing a stage with a held ticket in it applies at once, moves the ticket without asking and keeps its old hold (UX-60).
7. C · Decide offers "Send it on to Code review", then refuses it (UX-60 vs one-check-agent O5).

## Real but smaller
8. A · The card says only "Idle" after a tool error; the toast has the reason (UX-14, A11).
9. A · Tools shows amber permission warnings (UX-36 moved them to a one-time hint).
10. A · Windows toasts are titled "Windows PowerShell" with its icon.
11. B · A ticket's own agent line shows only after its stage starts (UX-11, UX-21).
12. B · The refused-drag toast doesn't say what it waits for (A42).
13. B · Palette says "Pause this project", no shortcut (canvas: "Pause shop ⇧⌘.").
14. B · Noyan's dependency change isn't listed as a CHANGED row in its panel (brief 2b keeps CHANGED rows).
15. C · A permission prompt shows only "Answer in terminal" on Overview, not the command (A22).
16. C · The send-back reason is posted as the user's comment (UX-55; gap analysis "to fix").
17. C · "Validate failed 1 times".
18. C · A failed merge check's pill says "Merge conflict · fixing".
19. C · What the agent did stays on the first round after a send back or restack (A20).
20. C · The conflict banner lacks what moved, "nothing was merged" and "1 of 3" (ticket-05b).
21. D · No "Ask the agent to write it…" in Pick a skill… or the missing-skill warning (UX-26, S7).
22. D · A stale "…is drafting a flow" line stays after Discard.
23. D · Quit Otaq from the tray asks nothing while agents work (A52); agents keep running, as the default.
24. D · The restart dialog has no unticked row for the signed-out tool; "Resume 0" (A49).

## Not a failure after all
25. D · Run has one command and one Open URL, not the drawn table: several apps already exist ("Add app" shows once a
    run command is set; the test project had none), and piece 21 kept today's rows on purpose.

## Decided 2026-11-10 (tuq:product-spec)
- 7 → decision 009 (Send it on on every check); 3 → decision 010 (the writer's ask in the Approve dialog).
- All 23 real failures go into one milestone before the release: tuq/plans/release-fixes.md.

## Extra check from a user report
- PASS · Send back with only a pasted-text chip is sent (tuq 007); the user's report is v0.1.0's old rule.
