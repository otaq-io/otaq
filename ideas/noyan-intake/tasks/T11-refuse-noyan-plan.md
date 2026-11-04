# T11. The service refuses a Plan from Noyan (N17)

**Depends on:** nothing new. **Runs in parallel with:** T9, T10, T12.

Read `ideas/noyan-intake/DECISIONS.md` first (N1, N10, N17).

## What to change

1. **Refuse a Plan from the Project agent** on `otaq create` and `otaq update` (`--plan`, `--plan-file`,
   `--plan-stdin`), and in `otaq create -f` files if they can carry a Plan (check the batch parser). Batches stay all
   or nothing. Use `Refusal::NotAllowed` and N17's text as a `pub const`:

   > Noyan files tickets; the flow's stages write the Plan

   Find where a ticket update or create is judged by actor (`crates/otaq-core/src/policy.rs`, the update/create paths
   in `crates/otaq-core/src/board/tickets.rs` and `service/board_requests.rs`, `crates/otaq-cli/src/write.rs`), and
   put the check in the service, not only the CLI. Clearing a Plan counts as writing one. Noyan's other edits to
   Backlog tickets (title, description, priority, deps) stay allowed.
2. **Ticket agents and the user** keep writing Plans as today (T4's tests in `plan_mid_build.rs` must keep passing).
3. **`otaq prime --project`** (`crates/otaq-core/src/project_agents/prime.rs`): if it tells Noyan anything about
   writing Plans, remove it.
4. **Tests:** Noyan's create with a Plan and its update with a Plan are refused with the exact text and write
   nothing; a `-f` batch with a Plan from Noyan creates nothing (if `-f` carries Plans); Noyan's description update
   still works; a ticket agent's and the user's Plan still work. Fix any existing test that had Noyan write a Plan
   (`grep -rn "plan" crates/*/tests/suite/ | grep -i "noyan\|project"`).

## Out of scope

The `noyan` skill (T2 already removed Plans from it).

## Done when

- `scripts/check.sh lint test` passes (apart from the known `naming_literals` failure).
- N17's text appears exactly once in the code, as the constant.
