# 106 · Installing tuq puts its commands on the user's PATH

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 064, 077, 102, 104

## Summary

Installing tuq, by Otaq or by hand following tuq's README, also puts `tuq` and `tuq-plan` in a folder on the user's
PATH, on every tool and system, so tuq's skills find their commands in Otaq's sessions and in the user's own terminal
alike.

## Decision

- On Claude Code the plugin's own `bin/` already does it; on Codex, Copilot and OpenCode tuq's install adds the
  commands to a folder on the user's PATH, on macOS, Windows and Linux.
- Updating tuq (decision 103) updates the commands with the skills, so a tool's skills and commands always match.
- The commands stay pre-approved in Otaq's sessions (decision 077).
- Disconnecting a tool leaves them, as it leaves tuq (decision 104).

## Why

Only Claude Code puts a plugin's `bin/` on the PATH, so on the other three tools tuq's skills would stop at "`tuq`:
command not found", in Otaq and in a plain terminal. Putting the commands only on Otaq's sessions' PATH was turned down
because tuq used on its own would still fail; dropping the commands and having agents number and index by hand was
turned down as error-prone.
