# T8. Clear stored choices that the new rules refuse (N14)

**Depends on:** nothing. **Runs in parallel with:** T1, T2, T3, T4, T7.

Read `ideas/noyan-intake/DECISIONS.md` first (N11, N12, N14).

## What to change

1. **A new migration**, `crates/otaq-core/migrations/0072_<name>.sql`, or the next free number when this lands.
   Never edit an existing migration. Read the header comments of the newest few to follow the house style: the
   first-line comment with the reason, `-- no-transaction` only if needed.
   - Delete from `ticket_stage_choices`:
     - rows with `set_by = 'project_agent'` (any `choice`);
     - rows with `set_by = 'agent' AND choice = 'skip'`.
   - **Only where the ticket has no run of that stage yet.** Find the stage-runs table and how
     `Target::ran` / "locked" is computed (`crates/otaq-core/src/board/stage_choices.rs`, `judge`), and mirror it in
     SQL.
   - Keep everything else: `set_by = 'user'`, agents' `agent`/`usual` choices, and anything on a started stage.
2. **History.** For each cleared choice, record an event on its ticket that the History reads as, for example,
   "Product spec: Noyan's skip cleared · only the user skips stages now". Find how ticket events are stored
   (`record_event`, `EventKind` in `crates/otaq-core/src/board/tickets.rs`). If an event can be inserted from SQL in
   the migration, do it there. If it needs Rust, use whatever one-time post-migration hook the codebase already has.
   If neither exists, stop and report rather than inventing a mechanism.
3. **Tests** (`crates/otaq-core/tests/suite/migrations.rs` and its fixtures). Build a database at the previous
   version that has:
   - Noyan's skip and agent pick on a Backlog ticket;
   - a ticket agent's skip and agent pick on an unstarted later stage;
   - a user's skip;
   - an agent `--by-user` skip;
   - a Noyan skip on a stage that already ran.
   After migrating, assert exactly which rows remain, and that the History events exist for the cleared ones.

## Done when

`scripts/check.sh lint test` passes.
