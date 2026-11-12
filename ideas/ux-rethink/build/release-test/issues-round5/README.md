# Round 5 · issues left after re-test 5

Left open by the Windows re-test of release-fixes 528d738a (`../RESULTS-retest-5.md`, screenshots
`../shots-retest-5/`) and by the round-4 piece agents' hand-backs. Logged 2026-11-11 for one more round after the
git-hooks milestone (tuq/plans/git-hooks.md) is built and merged into release-fixes. One file per issue.

State when logged: release-fixes at 528d738a, check.sh passing (3472 tests); round 4 (plan release-issues, pieces
24–33) merged and re-tested: 10 PASS, 28 PARTLY, 26 FAIL. Decisions so far: tuq/decisions/020–025 (round fixes),
026–039 (git-hooks).

## Must fix
- [01](01-restack-typed-onto-pasted-text.md) · A restack request is typed onto pasted unsent text after one Backspace
- [03](03-fixing-state-hides-prompts.md) · Prompts during "Merge check failed · fixing" show no Needs you
- [04](04-stale-waiting-message-and-unsent-footer.md) · A "1 message waiting" that never goes, and "Unsent text" over an empty line

## Should fix
- [05](05-login-expired-sign-in-late.md) · A login-expired ticket shows Sign in… only after the tools are re-checked

## Decide (fix now or next milestone, or how)
- [02](02-queued-codex-prompt-not-needs-you.md) · A Codex prompt queued behind another never reads Needs you (several open prompts per session)
- [06](06-focused-terminal-card-working.md) · With the terminal focused, the card says Working while the agent waits
- [07](07-ctrl-c-wipes-codex-draft.md) · Ctrl+C in a Codex terminal on Windows wipes the user's draft

## Can wait
- [08](08-held-ticket-first-stage-in-small-views.md) · Small views still name the first stage for a held ticket that skips it
- [09](09-abandoned-skill-request-blocks.md) · An abandoned "Ask the agent to write it" blocks a new one for another stage

## Re-test
- [10](10-re-test-gaps.md) · Checks not yet seen in the real app (held column, remove note with tickets, PR paths with hooks, piece 23's answer sessions)

## Small cleanups
- The Rust doc comment on `AgentToolSignIn` still says its `tab` is "the tab to open in the Terminals drawer" (piece 25
  stopped using it).
- `crates/otaq-desktop/ui/src/screens/flow_board/check_first.view.json` holds the old remove-note text (regenerated
  only with `OTAQ_WRITE_FIXTURES`).
- In the VM: SHOP-33's worktree (uncommitted README change) is listed in Cleanup; shop's main has setup commits
  87d39a7 and re-test merges on top.
