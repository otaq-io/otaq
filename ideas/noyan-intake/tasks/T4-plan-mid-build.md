# T4. The Plan can be written and grown during the build stage (enables N8, N9)

**Depends on:** nothing. **Runs in parallel with:** T1, T2, T3, T7. **Blocks:** T5.

Read `ideas/noyan-intake/DECISIONS.md` first (N8, N9).

N8 and N9 rely on two things today's skills never do. Find out whether Otaq allows them, and make it allow them where
it doesn't.

## Case A: the Dev agent writes a Plan on its own started ticket

The Dev agent, in its build stage on a ticket **with no Plan**, writes a Plan of pieces on its own ticket
(`otaq update $OTAQ_TICKET --plan-file …`) and then starts helpers (`otaq helper start 1 …`). Check:

- Is the update accepted? (`crates/otaq-cli/src/write.rs` says an agent may update its own ticket, T-0065.)
- Does `otaq prime` then show the Pieces?
- Does `otaq helper start` accept the pieces?

## Case B: the agent in charge grows a Plan while helpers run

With helpers running, the agent in charge adds pieces to the Plan, and may shrink a piece's description to what was
already built. Check:

- Is the update accepted?
- Does the "The plan changed: …" message reach the session?
- Can new pieces start?
- Is a running piece left alone when only its text changes?

## What to do

1. Write integration tests for both cases in `crates/otaq-core/tests/suite/`. Look at the existing helper and plan
   suites (`grep -ln "helper" crates/otaq-core/tests/suite/*.rs`) and follow their fixtures.
2. If a case is refused or misbehaves, make the smallest service change that allows it. Keep the existing refusals for
   other actors: a ticket agent still can't edit another started ticket.
3. **Write down the exact commands and messages** that the agent sees, in
   `ideas/noyan-intake/tasks/T4-findings.md`. T5 writes the skill text from it.

## Out of scope

Skill text (T5).

## Done when

- `scripts/check.sh lint test` passes.
- `T4-findings.md` lists, for each case, the working command sequence and anything the skill must warn about.
