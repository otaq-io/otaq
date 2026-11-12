# 05 · A login-expired ticket shows Sign in… only after the tools are re-checked

**Kind:** bug · **Release:** should fix · **From:** re-test 5 NOTE-1

## What happens
- NOTE-16 stopped "login expired"; its sheet's callout read "Sign in to Claude Code, then Resume" with only Resume and
  Answer in terminal. Sign in… appeared only after opening Settings → Agents (which re-checks the tools).

## Evidence
Screenshots in `../shots-retest-5/`: `c25-sheet-no-sign-in-before-recheck.jpg`, `c25-settings-claude-not-signed-in.jpg`, `c25-sheet-sign-in-button.jpg`

## Where to start
Where the stop reason sets the tool's state (agent_tools/connected.rs) and the callout's buttons (tool problems).

## Open questions
- A "login expired" stop marks the tool signed out at once (or re-checks it)?
