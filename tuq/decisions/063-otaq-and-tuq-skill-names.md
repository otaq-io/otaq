# 063 · Otaq's skills are named otaq: (workflow) and tuq: (generic)

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 062, 064

## Summary

Otaq's workflow skills are the `otaq` plugin's, written `otaq:<name>`; the generic skills are tuq's, written
`tuq:<name>`. A bare skill name in a flow is always the user's own skill, so a release that adds a skill can never
take over a user's stage.

## Decision

- Workflow skills: `otaq:work-ticket`, `otaq:validate`, `otaq:commit`… from `otaq-io/otaq-skills` (decision 057).
- Generic skills: `tuq:product-spec`, `tuq:security-check`… from `otaq-io/tuq`.
- `validate` alone in a flow means the user's skill called `validate`.
- This replaces "a name that matches one of Otaq's shipped skills always means Otaq's skill" (S3 in
  `ideas/no-skill-copies/DECISIONS.md`).

## Why

Once releases can add skills at any time, a name that today is the user's could later match one of Otaq's and its
stage would quietly switch. Letting the user's skill win on a clash was turned down (many users have their own
`validate` or `commit`). The workflow skills can't share the name `tuq`, because a session also loads the user's own
tuq (decision 080); the user chose `otaq` for them, over `otaq-flow` and `tuq-otaq`, though an earlier point said no
skill would say `otaq:` again. Was S15.
