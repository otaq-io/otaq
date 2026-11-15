# 096 · A workflow-skills release needs the newest released Otaq unless lowered on purpose

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Supersedes:** 060
- **Related:** 057, 058, 066, 076

## Summary

When the maintainer publishes Otaq's workflow skills, the publish command proposes the newest released Otaq as the
release's "needs Otaq ≥"; the maintainer may lower it on purpose. A forgotten minimum then keeps the release away from
older apps instead of sending them skills that call commands they don't have.

## Decision

- The default minimum is the newest Otaq version released when the skills are published; the publish command shows it
  and the maintainer may set a lower one, for a fix that uses nothing new in `otaq`.
- It replaces only decision 060's default ("keeps X as before unless the maintainer raises it"). The rest of 060
  stands: each app takes the newest release whose range covers it, an older app stays on the last one that works with
  it, and tuq releases carry no range.

## Why

A minimum kept from the last release unless raised by hand breaks older apps' stages the day the maintainer forgets:
such a release is "usable" (decision 066), so nothing stops it. Making the safe value the default turns a forgotten
step into older apps missing a fix, not failing. Checking every `otaq` command the skills use against each released
app was turned down as more to build for now; keeping it manual was turned down for the reason above.
