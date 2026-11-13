# 09 · An abandoned "Ask the agent to write it" blocks a new one for another stage

**Kind:** bug · **Release:** can wait · **From:** piece 24's hand-back (round 4)

## What happens
- Piece 24 refuses a write-skill for another stage while the same agent was asked for a stage and hasn't named a
  folder yet. If that earlier request was abandoned before a folder was named, a new request for a different stage is
  refused until the old one is stopped.

## Evidence
Screenshots in `../shots-retest-5/`: none.

## Where to start
crates/otaq-core/src/flow/write_skill.rs `asked_for_another`.

## Open questions
- Should a newer "Ask the agent to write it" replace the older request?
