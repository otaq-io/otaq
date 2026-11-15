# 083 · A tuq skill the session doesn't have holds the stage

- **Date:** 2026-11-14
- **Status:** Partly superseded by 103
- **Related:** 060, 061, 076, 080

## Summary

A `tuq:` skill is checked like any other: when the tuq a session would run doesn't have it, the stage doesn't start
and waits in Needs you, as a stage whose skill is missing does today.

## Decision

- It applies to Otaq's bundles (an older bundle, the built-in skills) and to a user's own tuq install (decision 080).
- The message names the skill and where it is missing: "Otaq's skills don't have `tuq:update-docs` yet" or "Your tuq
  doesn't have `tuq:update-docs`".
- **Check again** also checks for a newer bundle at once.
- A skill Otaq types into a running session (a hand-off) is checked the same way before it is typed.

## Why

With bundles and user installs of different ages, a `tuq:` name in a flow (from a teammate, an import or a newer
`flow` skill) may not exist where it runs (gap G12). Holding it follows the rule every other missing skill already
has, so nothing was asked.
