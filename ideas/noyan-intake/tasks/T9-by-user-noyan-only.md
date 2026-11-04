# T9. Only Noyan uses `--by-user`; a ticket agent sees only stages it can change (N15, N20)

**Depends on:** T1 (merged). **Runs in parallel with:** T10, T11, T12.

Read `ideas/noyan-intake/DECISIONS.md` first (N11, N12, N15, N20).

## What to change

1. **`crates/otaq-core/src/board/stage_choices.rs`, `judge`.** Today `Chooser::User || by_user` is accepted early.
   Change it so `by_user` counts only for `Chooser::ProjectAgent`. A `Chooser::TicketAgent` choice with `by_user` is
   refused with `Refusal::NotAllowed` and N15's text, as a `pub const` next to the two T1 constants:

   > Only Noyan passes on the user's choices: pick another agent with --reason, or ask the user to set it

   - Find every other place the request's `by_user` flag is honoured for stage choices (`grep -rn by_user crates/`;
     for example `otaq create` / `otaq update` in `crates/otaq-cli/src/write.rs`, `service/board_requests.rs`, how a
     choice is stored as `set_by = 'user'`) and make sure a ticket agent's `--by-user` can't reach any of them.
   - Helpers are ticket agents for this rule.
   - Update the module doc comment.
2. **`crates/otaq-core/src/prime.rs`, `render_choices`, `AgentReader::TicketAgent` (N20).** List only the stages
   where the ticket agent can pick another agent (an option that isn't the stage's usual agent and isn't `skip`).
   When none remain, return an empty section, the same way the function does when the reader may set nothing.
3. **Skills.** `grep -rn "by-user" plugin/skills` and make sure no ticket-stage skill tells its agent to use
   `--by-user`. The `noyan` skill keeps it.
4. **Tests.** T1 added "an agent's `--by-user` skip still works" style cases for ticket agents
   (`crates/otaq-core/tests/suite/stage_choices.rs`, `crates/otaq-cli/tests/suite/stage_choices.rs`, the unit test
   `the_choice_rule`): turn them into refusals with the exact text. **Add:**
   - a ticket agent's `--by-user` skip is refused, nothing is written;
   - a ticket agent's `--by-user` agent pick is refused;
   - Noyan's `--by-user` skip and pick still work and are stored as the user's;
   - a ticket agent's later-stage agent pick with `--reason` still works;
   - `prime` for a ticket agent leaves out a stage whose only option is `skip`, and leaves the section out when no
     stage remains.

## Out of scope

Choices already stored as `user` (they stay). The Plan refusal (T11).

## Done when

- `scripts/check.sh lint test` passes (apart from the known `naming_literals` failure).
- N15's text appears exactly once in the code, as the constant.
