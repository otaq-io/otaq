# 056 · Warm builds kept before decision 054 are thrown away once

- **Date:** 2026-11-13
- **Status:** Accepted
- **Related:** 054 (ticket OTAQ-5)

## Summary

The first time a version of Otaq with decision 054 runs, it throws away every warm build it kept before, in every
project, so their disk space comes back at once. The next ticket starts from the user's own folder (decision 055) or
builds from scratch, so it is slower once; from then on kept builds stay small.

## Decision

- Once per install, on the first run with decision 054: all kept warm builds are cleared, as Clear… in Cleanup does.
  Worktrees of tickets in progress are not touched.
- No question, no setting; Cleanup simply shows no kept build until the next ticket finishes.

## Why

The kept builds were already huge (211 GB for the otaq project on 2026-11-13). The user chose to get that space back
right away over trimming each build the next time it is used, accepting one slow first build per project.
