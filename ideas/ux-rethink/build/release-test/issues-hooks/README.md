# Git hooks · issues before the release

Found by the Windows exploration of how Otaq behaves in a repository with git hooks (`../RESULTS-hooks.md`, build
release-fixes 5e8e9fc7, screenshots in `../shots-hooks/`). The maintainer, 2026-11-11: "we need to improve the UX for
the hooks and fix [these] before we release". One file per issue, ranked by how much it would bite a real user; each
ends with the open questions a product spec has to settle.

What already works and must stay: Otaq runs the user's own git with the user's hooks and never passes `--no-verify`;
a refused hook leaves main, the branch and the worktree clean; agents read a hook's output, don't bypass it, and ask.

## Next steps
Run a product spec (tuq:product-spec) over these as one feature, "Otaq works with a project's git hooks": one question
at a time, decisions in tuq/decisions/. Then a tech spec where the design needs one, then a milestone plan
(tuq/plans/) and its orchestrator prompt. The overall UX question to answer first: how should Otaq show a hook at work
and a hook's refusal, everywhere it runs git (start, merge, checkpoint, rebase, agents' commits and pushes)?

## Issues
- [01](01-own-commits-ignore-message-rules.md) · Otaq's own commits can't pass a project's commit-message rules
- [02](02-pre-rebase-blocks-every-merge.md) · A pre-rebase hook (or a no-rebase team) makes Approve & merge impossible
- [03](03-push-guidance-and-outcome.md) · Agents' pushes: "Never push", pre-push hooks and the real result
- [04](04-hook-output-lost.md) · A hook's refusal shows only git's last line, in a toast that vanishes
- [05](05-merge-message-edit-lost.md) · Approve & merge forgets the edited message after a refusal
- [06](06-hook-changes-merged-commit.md) · A reformatting pre-commit changes what lands on main after approval, silently
- [07](07-no-progress-or-timeout-for-hooks.md) · No progress, Stop or time limit while hooks run
- [08](08-retry-start-skips-post-checkout.md) · Retry start skips a post-checkout hook that failed
- [09](09-terminal-hooks-cant-pass.md) · Hooks that read from the terminal can never pass, and nothing says why
- [10](10-pre-merge-commit-never-runs.md) · A pre-merge-commit hook never runs
- [11](11-post-merge-in-users-checkout.md) · post-merge runs in the user's own checkout without a word

## Not tried (needs a PR provider)
Approve & open PR and Push to PR with pre-push and commit-msg hooks: the VM has no gh/az and a local origin. Re-test
them on a machine with a provider before the plan is final (see 03).
