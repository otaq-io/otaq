# 101 · Every session uses the newest skills; no old release is kept

- **Date:** 2026-11-14
- **Status:** Superseded by 102
- **Supersedes:** 061, 067, 069, 078, 084, 091
- **Related:** 066, 076, 097, 099

## Summary

Otaq keeps one copy of each skills repository's newest usable release. Every new session and every resume uses it;
no session keeps the release it started with, and nothing older is kept, on any tool.

## Decision

- A new release replaces the previous one as soon as Otaq has it in full and has checked it (decision 066); the
  switch is never half-done. The previous release's files are deleted as soon as the new one is in place: no ticket
  or session waits on them.
- New sessions, resumes, a Send back, a restart and every skill Otaq types into a running session use the newest
  release. A session open across a release may therefore follow two versions of a skill; that is accepted.
- The same on every tool: Codex gets one copy, updated in place like the hooks plugin, not one per session (replaces
  decision 091).
- The skills built into an app are used only while nothing newer is usable (decision 076); after an app update the old
  app's skills are gone with it (replaces decision 078).
- A bad release reaches running sessions at their next resume or typed skill, like new ones; it is still undone by
  publishing again (decision 067).
- A ticket's History names the skills each session started on (decision 069).
- A release that renames a step or changes a command's form keeps the old form working for a while, as decision 097
  does for `otaq`, so a session spanning a release rarely breaks.

## Why

Keeping each session on its release (decision 061) protected only a session that spans a release, reads a changed
file afterwards, and meets a real conflict: rare, since stages are short and most releases are wording fixes. It cost a
store of old releases, a record per session, deletion rules, and on Codex an install per release with a flag on every
launch and resume, which the hands-on check showed is easy to get wrong. Keeping releases per session until the next
app update was turned down: it kept all of that machinery and still mixed versions across an app update.
