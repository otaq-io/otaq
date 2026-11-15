# 104 · tuq is part of Otaq: Otaq may install, update and use the user's tuq

- **Date:** 2026-11-14
- **Status:** Accepted
- **Supersedes:** 062, 090
- **Related:** 102, 103

## Summary

tuq is Otaq's own product, so the rule that Otaq installs and changes nothing outside itself doesn't apply to it: Otaq
may install tuq, update it and use the user's tuq install on every tool. The user's own skills stay untouched.

## Decision

- On every agent tool Otaq supports, Otaq may install tuq, update it (decision 103) and run the user's tuq in its
  sessions (decision 102), whether the user installed it or Otaq did.
- tuq doesn't count as one of the user's own skills: "a user's skills stay where the user keeps them, untouched"
  (decision 062) and "the user's own tuq install … is never touched" (decision 090) no longer cover it.
- Everything else stays out of bounds: Otaq installs, changes or removes none of the user's own skills, and nothing
  else outside its own folders beyond what earlier decisions allow (folder trust, its hooks).
- Disconnecting a tool leaves tuq installed there, whoever installed it: it is an ordinary install the user's own
  terminal uses too. The disconnect removes Otaq's hooks and workflow skills and their trust (decision 090), so a plain
  session of that tool shows no trace of Otaq apart from tuq.

## Why

The user's rule that Otaq doesn't install anything outside itself was written for other people's skills and tools.
tuq is Otaq's own product and works as part of it, and decisions 102 and 103 already have Otaq install and update it;
writing the exception down keeps the rule and the behaviour from contradicting each other.
