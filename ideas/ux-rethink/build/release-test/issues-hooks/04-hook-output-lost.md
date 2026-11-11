# 04 · A hook's refusal shows only git's last line, in a toast that vanishes

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 4, scenarios 2, 4, 7

## What happens
- Failures of Otaq's own git steps become a toast with `git <args> failed: <stderr>` cut to git's last line: "The
  pre-rebase hook refused to rebase." loses the hook's reason.
- The toast shows the full Windows hook path, with the user name in it.
- Nothing is written on the ticket's History: two refused merges left no trace once the toast was closed (only a WARN
  in the service log).
- The checkpoint error names the wrong hook (see 01).

## Evidence
Screenshots in `../shots-hooks/`: `s2-commitmsg-rejects-merge-toast-zoom.jpg`, `s2-interactive-precommit-fails-at-once-zoom.jpg`,
`s4-prerebase-toast-zoom-hook-reason-lost.jpg`

## Where to start
`Git::run` error text, the Approve & merge failure toast, History events.

## Open questions for the product spec
- A lasting entry on the ticket (History and/or the sheet) with which hook refused and its full output, and "Show
  output". Paths shortened (`~\…`).
- Can Otaq tell which hook refused (git doesn't say for commit-msg vs pre-commit)? Run the hooks itself first, or
  parse?
