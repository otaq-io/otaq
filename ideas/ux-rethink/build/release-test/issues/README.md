# Issues before the release

Found by the Windows re-test of `release-fixes` at 7f36bd79 (`../RESULTS-retest.md`, screenshots in
`../shots-retest/`) and by the piece agents while they worked. These are to be fixed or decided before the release.
One file per issue.

## Must fix (re-test FAIL)
- [01](01-restart-dialog-not-installed.md) · Restart dialog calls an installed, signed-in tool "not installed" → "Resume 0"
- [02](02-bom-skill-stage-has-no-warning.md) · A stage whose Codex skill has a byte order mark shows no warning

## Must re-test
- [03](03-claude-code-checks-not-tested.md) · Checks that need Claude Code signed in (piece 3 / failure 8, piece 4's row, Sign in…, Spec first stages)

## Bugs found in the re-test
- [04](04-stale-writer-request-after-send-back.md) · Message-writer request reaches the Dev session after a send-back
- [05](05-still-writing-with-no-writer.md) · "Still writing the message…" forever with no writer session
- [06](06-no-running-chip-after-keep-running.md) · No "1 running" chip after Keep them running
- [07](07-working-while-codex-idle.md) · "Working · in the background" while Codex sits idle
- [08](08-merge-check-dialog-empty-output.md) · Merge-check failure dialog has an empty output box
- [09](09-written-skill-left-after-cancel.md) · Cancel leaves the skill the agent wrote; the stage keeps warning

## Wording and smaller things
- [10](10-draft-line-uses-stage-id.md) · Draft line says "update-the-docs" instead of the stage's name
- [11](11-new-agent-model-list-hides-claude-code.md) · New agent → Model leaves out a signed-out Claude Code
- [12](12-codex-skill-named-with-slash.md) · Codex skill named "/skill" instead of "$skill"

## Needs a decision or a design
- [13](13-restart-dialog-count-and-sign-in.md) · Restart dialog title count and Sign in… (triage items 34 and 11)
- [14](14-noyan-changed-row-says-nothing.md) · Noyan's CHANGED row doesn't say what changed (no "now waits for … · Undo")

## Seen by piece agents, not in the re-test
- [15](15-codex-needs-you-lag.md) · Codex "Needs you" lags after a command is approved
- [16](16-toast-click-does-not-open-otaq.md) · Clicking a Windows notification doesn't open Otaq
- [17](17-parked-from-newest-first.md) · Probably the same newest-first bug in `parked_from`
- [18](18-flaky-worktree-skills-test.md) · Flaky test under load in `scripts/check.sh`

## To confirm
- [19](19-choices-made-by-piece-agents.md) · Choices the piece agents made that the plan didn't spell out

Not here: otaq-d6's base-commit staleness fix (its own branch). Ask that session where it stands.
