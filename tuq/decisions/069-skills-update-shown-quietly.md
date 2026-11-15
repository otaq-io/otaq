# 069 · A skills update is shown quietly, with a note for users

- **Date:** 2026-11-14
- **Status:** Partly superseded by 100, 101, 102
- **Related:** 061, 068, 080, 084

## Summary

No pop-up when skills change. Settings → About and each session in a ticket's History say which skills ran, and the maintainer writes a short note for users at publish.

## Decision

- Settings → About, under the app version: "otaq skills 2026.11.14 · tuq 2026.11.12 · What changed"; What changed
  opens each release's note.
- A ticket's History: each session row names the skills it ran ("Session · Dev · Strong · otaq skills 2026.11.14 ·
  tuq 2026.11.12").
- Versions are dates; a second release on the same day gets ".2" (2026.11.14.2). Skills built into the app read "built
  in with Otaq 0.3.0"; the user's own tuq install reads "tuq 2026.11.12 · yours" (decision 080).
- The publish command lists the skills changed since the last bundle and asks for the note, a line or two written for
  users, pre-filled from that repository's commit messages since its previous release for the maintainer to edit. The note is saved
  in the signed bundle.

## Why

Skills change agent behaviour, so a user (or the maintainer, reading a report) must be able to tell when they
changed. A toast on every publish was turned down as noise. Was S25.
