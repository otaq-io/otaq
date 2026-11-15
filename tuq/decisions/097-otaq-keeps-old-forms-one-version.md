# 097 · otaq keeps an old command form working for one app version

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Related:** 057, 096, 101

## Summary

When an Otaq update changes or removes an `otaq` command, flag or `prime` section that Otaq's workflow skills used, the
old form keeps working in that update; the next app version may drop it. A conversation resumed across one app update
can still finish what its earlier instructions say.

## Decision

- Covered: every `otaq` command and flag, and every part of `otaq prime`'s and other commands' output, that the
  workflow skills of the previous app version (its built-in ones or a release it used) rely on.
- In the update that replaces a form, the old form keeps working and may add a short note naming the new one; the
  version after it may remove the old form.
- Applies to Otaq's workflow skills only; tuq never calls `otaq` (decision 094).

## Why

Every session and resume uses the newest skills (decision 101), so old workflow skills never run from disk on a newer
app; only the instructions already in a resumed conversation can be older, and one app version covers them. Keeping
old forms for 6 months was turned down as old code carried for nothing; promising nothing, with an error naming the new
form, was turned down because a resumed stage could stumble.
