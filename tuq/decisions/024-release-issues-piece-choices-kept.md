# 024 · Choices the release-issues piece agents made are kept

- **Date:** 2026-11-11
- **Status:** Accepted

## Summary

The release-issues plan left some behaviours open and its piece agents chose them. The user confirmed these as the
product's behaviour, except piece 14's Undo row (decision 023).

## Decision

- Restart dialog: while a tool's first check runs, its row says "Checking Codex…" (piece 1); the title also counts
  agents still waiting for your answer (piece 2); signing in from a waiting row ticks it and resumes it with "carry
  on" (piece 3).
- Approve: a message writer from an earlier run is asked again when Approve opens; if it can't be, the dialog opens on
  the plain message (piece 6).
- The running chip's list rows read "Developer · 26m" (piece 7).
- Dragged back from Parked, the agent gets "You were parked and are now back. Carry on where you left off…" (piece 8,
  decision 020).
- Cancel in Edit flow removes only skill files the agent wrote that git doesn't track yet; a skill the user had or
  changed stays (piece 10).
- A change to a stage's choice is a CHANGED row (piece 13); the ticket's title is the row's tooltip (piece 15).
- The skill warning reads "Pick one, or Ask the agent to write it" (piece 16); Edit flow's line reads "Needs you · X
  wants your permission: …" (piece 17).
- The installer registers the `otaq://` link for the current user in the Windows registry (piece 20); uninstalling
  removes it.
- Enter answers a checker's permission prompt on every tool (piece 23).
- `otaq daemon start` keeps its 10-second wait for the service.
