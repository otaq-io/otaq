# 108 · Pick a skill… marks Otaq's built-in skills with a badge and their own group

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 063, 074, 102, 104

## Summary

In the flow editor's Pick a skill…, Otaq's own skills (the `otaq:` workflow skills and tuq's `tuq:` skills) sit in a
group headed "Otaq" with Otaq's icon, and each of their rows carries a small "Otaq" badge, so the user sees at a glance
which skills come with Otaq and which are their own.

## Decision

- The user's own skills come first, unmarked, as today.
- Below them, a group heading "Otaq" with Otaq's icon; each row in it shows the skill by its namespaced name
  (`otaq:validate`, `tuq:product-spec`) with an "Otaq" badge, and its second line says what it does.
- tuq's skills count as Otaq's here (decision 104), whoever installed tuq.
- Searching keeps the badge on every Otaq row it shows.

## Why

The user asked for a clear sign that a skill is a built-in one from the Otaq organization: today the only hint is a
plain "Otaq's ·" at the start of the second line. Marking only the `otaq:` skills (tuq with a badge of its own) and
marking with an icon alone were turned down as less clear for new users.
