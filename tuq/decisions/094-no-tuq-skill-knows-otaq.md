# 094 · No tuq skill knows Otaq, step-back included

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 081, 089, 093

## Summary

Every tuq skill, not only the three decision 081 lists, works without Otaq: `step-back` loses its Otaq branch and asks
the plain-chat way only. The same holds for every tuq skill added later (`security-check`, `update-docs`,
`reproduce`).

## Decision

- `step-back` no longer checks `$OTAQ_TICKET` or calls `otaq ask human`; it offers its choices as it does in a plain
  chat.
- Inside an Otaq stage, Otaq's workflow skill says how a tuq skill's questions reach the user in Otaq, as it does for
  `product-spec`, `tech-spec` and `milestone-plan` (decision 081).
- The rule is "no tuq skill or `tuq` command names `otaq` or `$OTAQ_*`", not a list of skills, so a new tuq skill
  follows it without another decision.

## Why

Decision 081 named three skills, and `step-back` kept its own Otaq branch, so tuq still depended on how `otaq ask
human` works. One rule for every tuq skill keeps tuq usable on its own and lets any version of a user's tuq run in
Otaq. Keeping step-back's branch was turned down for that reason.
