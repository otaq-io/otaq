# 038 · The post-merge hook in the user's folder is shown, its output kept in History

- **Date:** 2026-11-11
- **Status:** Accepted
- **Related:** 026, 027

## Summary

While a merge's last step runs the user's post-merge hook in their own repository folder, Otaq shows it running
("running your post-merge hook in ~/shop-web · 20 s"). Its output, whether it passed or failed, goes into the ticket's
History. There is no Stop and no extra ticket state.

## Decision

- While the post-merge hook runs in the user's own folder, the card and the Merging dialog show it, with where it runs
  and how long: "running your post-merge hook in ~/shop-web · 20 s" (027).
- It has **no Stop**: the merge has already landed when it runs, so there is nothing to undo.
- Its output is a line in the ticket's History, whether it passed or failed. A failure doesn't change the ticket: it is
  merged, with no Needs you and no special state.
- With no post-merge hook, nothing extra is shown.

## Why

The release test (`ideas/ux-rethink/build/release-test/issues-hooks/11`) found the post-merge hook running in the user's
own folder after every merge (for example an `npm install`) without a word from Otaq. Showing it while it runs explains
a slow end to the merge. A first version of this decision (agreed earlier the same day, never committed) added Stop and
a "merged, but the post-merge hook failed" state. On a step back the user dropped them: the merge has already landed,
so the History line carries what matters. Also turned down: a History line only (a slow install would look like a
stuck merge).
