# 089 · Inside Otaq, no skill starts by itself

- **Date:** 2026-11-14
- **Status:** Accepted
- **Related:** 080, 081

## Summary

In Otaq's sessions an agent never starts a tuq skill on its own: a skill runs only when Otaq runs it for a stage or a
person types it. Outside Otaq, in people's own terminals, tuq skills behave as tuq defines.

## Decision

- Applies to every Otaq session: a stage's agent, a checker, Noyan and its flow drafting, a helper.
- Describing a feature in Noyan's chat ("I want users to…") never starts Product spec.
- A skill that was started (by Otaq for the stage, or typed by the user) may call the skills it names, as anywhere
  else: `grill-me` → `grilling`, `product-spec` → `step-back`.
- How: every Otaq session is told, in Otaq's own rules, "never start a skill on your own initiative; a skill that was
  started may call the skills it names". Otaq's own copies keep their person-only marks (every workflow skill except
  `commit`), and its copies of the entry-point skills `product-spec`, `tech-spec` and `milestone-plan` get them too.
  A skill that other skills call (`commit`, `step-back`) is never marked, and a user's own skills are never changed.
- No tool-level block on the skill tool: it can't tell a chain from a self-start, and would break `grill-me` and the
  like. A user's own tuq install on a tool without marks relies on the rule alone.
- tuq's own behaviour in a plain terminal is unchanged.

## Why

With tuq loaded in Otaq's sessions, Product spec could start in Noyan's chat or a Dev session and write product
decisions nobody asked for (gap G8); for users with tuq installed this already happens today. The user: "In Otaq,
no." A Claude Code permission rule blocking the skill tool was turned down because skills call other skills.
