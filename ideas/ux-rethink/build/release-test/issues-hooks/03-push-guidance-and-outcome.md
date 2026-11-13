# 03 · Agents' pushes: "Never push", pre-push hooks and the real result

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 3, Pre-push section

## What happens
- Otaq never runs `git push`; every push is an agent's own command (a user's request, Approve & open PR's `open-pr`
  skill, Push to PR's `push` skill).
- The work-ticket and commit skills say "Never push": Codex refused a push the user asked for in a comment and only
  pushed after "I override the no-push rule" via Tell now.
- The push and open-pr skills say nothing about pre-push hooks (wait for them, report the real result, never
  `--no-verify`, ask the user when one refuses). Only the commit skill mentions `--no-verify`.
- A slow (50 s) pre-push: Codex stopped waiting at ~30 s, the push then succeeded, and its hand-off said "the push
  reported no completion; no upstream". The ticket went on with a wrong report.
- A failing pre-push was reported honestly with no bypass; an interactive one failed at once ("/dev/tty: No such
  device or address").
- Approve & open PR couldn't be tried (no PR provider in the VM): from the skills, a refused push would come back as
  the agent's Asks you, after Otaq already renamed the branch and reworded the commits.

## Evidence
Screenshots in `../shots-hooks/`: `pp-agent-refuses-push-from-comment.jpg`, `pp-a-agent-pushes-on-request.jpg`,
`pp-a-prepush-refuses-agent-reports-no-bypass.jpg`, `pp-a-prepush-output-in-session.jpg`,
`pp-c-slow-prepush-agent-working-29s.jpg`, `pp-c-slow-prepush-codex-moves-on-push-still-running.jpg`,
`pp-c-push-succeeded-but-handoff-says-no-completion.jpg`, `pp-d-interactive-prepush-fails-no-tty.jpg`,
`pp-b-open-pr-disabled-no-provider.jpg`

## Where to start
`plugin/skills/work-ticket`, `commit`, `push`, `open-pr` skills; `otaq pr opened`.

## Open questions for the product spec
- When may an agent push: never unless the user asks, or only in the delivery skills? How does a user's request
  override "Never push"?
- Should Otaq check the outcome itself (upstream / `git ls-remote`) after `otaq pr opened` or a stage end, rather than
  trust the agent's text? Should Otaq run the push itself in delivery, so it can show progress and the hook's output?
- Re-test Approve & open PR with a pre-push hook on a machine with gh or az.
