# 118 · A Codex session at its own sign-in screen asks the user to sign in there

- **Date:** 2026-11-16
- **Status:** Accepted

## Summary

When a Codex agent's session stops at Codex's own sign-in screen (its sign-in expired), the ticket goes to Needs you
with "Codex needs you to sign in"; the user signs in in that terminal and the agent goes on with the ticket.

## Decision

- Otaq recognises Codex's sign-in screen in the session and reads it as Needs you, not Working.
- The ticket's card and Needs you say "Codex needs you to sign in"; opening it shows the session's terminal.
- The session is kept: once the user signs in there, the agent carries on where it was.
- Settings → Agents stops saying Codex is signed in once a session showed its sign-in screen, until a Codex session gets
  past it or a Sign in… terminal for Codex ends.

## Why

Found in the 0.3 Windows regression test (F-E2-4): with an expired ChatGPT sign-in the card said Working for minutes and
Settings said signed in. Turned down: stopping the session and holding the ticket until a sign-in from Settings.

## Changelog

- 2026-11-16: refined: a Sign in… terminal for Codex that ends also clears "not signed in"; otherwise a user who signs
  in from Settings after stopping the waiting session could never start Codex again (found while building it).
