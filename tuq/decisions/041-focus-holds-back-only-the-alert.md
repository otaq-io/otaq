# 041 · Being in the terminal holds back only the alert, never the card's state

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 040

## Summary

While the user has an agent's terminal focused, Otaq holds back that agent's alerts (sound, notification, strip
ping), but its card and pill always show the agent's real state, Needs you included.

## Decision

- Focus on an agent's terminal ("you're here, so no alert") holds back alerts for that agent only.
- The card, the sheet's pill and the session's state show the same, real state at all times: an agent waiting at a
  prompt reads Needs you on the card even while its terminal is focused.

## Why

Re-test 5 (round-5 issue 06): with the terminal focused, a Codex prompt left the card "Working" while the pill said
"Needs you" for 12 s+. The maintainer chose that the board never says Working for a waiting agent.
