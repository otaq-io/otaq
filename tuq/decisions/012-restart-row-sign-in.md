# 012 · A signed-out row in the restart dialog has Sign in…

- **Date:** 2026-11-10
- **Status:** Accepted
- **Related:** 011

## Summary

In the restart dialog, a row that can't resume because its tool is signed out has a Sign in… action. Once the user
signs in, the row is ticked and the Resume button counts it.

## Decision

- A row unticked because its tool is signed out shows its reason and a **Sign in…** action, which starts the same
  sign-in as Settings → Agents.
- When sign-in succeeds, the row becomes ticked and the button's count goes up by one ("Resume 2" → "Resume 3"). The
  title doesn't change: it already counts every stopped agent (011).
- If sign-in fails or the user drops it, the row stays unticked with its reason, and the user can try again.
- Rows that can't resume for another reason (the tool isn't installed, say) get no Sign in….

## Why

The user can fix the problem where they see it instead of leaving the dialog, going to Settings → Agents and then
resuming the red card by hand. Triage item 11 (assumption A49, UX-37) asked for it back; piece 4 of the release fixes
left it out.
