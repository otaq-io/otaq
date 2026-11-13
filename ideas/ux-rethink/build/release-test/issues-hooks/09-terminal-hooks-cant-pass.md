# 09 · Hooks that read from the terminal can never pass, and nothing says why

**Kind:** missing · **Release:** must fix before the release (maintainer, 2026-11-11) · **From:** ../RESULTS-hooks.md, missing 9, scenarios 2 and 3

## What happens
- Otaq's git steps run with stdin `/dev/null`; an interactive pre-commit or pre-push ("commit anyway? [y/N]",
  `exec < /dev/tty`) fails at once with "/dev/tty: No such device or address", in Otaq's merge and in agent sessions.
  It doesn't hang, but the user isn't told it's a hook needing a terminal.

## Evidence
Screenshots in `../shots-hooks/`: `s2-interactive-precommit-fails-at-once.jpg` (+ `-zoom`), `pp-d-interactive-prepush-fails-no-tty.jpg`

## Where to start
`Git::run` error handling.

## Open questions for the product spec
- Recognise the `/dev/tty` failure and say "this hook reads from the terminal; Otaq can't answer it"? Anything more?
