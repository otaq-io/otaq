# T1. The service refuses Noyan's own stage choices (N11, N4)

**Depends on:** nothing. **Runs in parallel with:** T2, T3, T4, T7.

Read `ideas/noyan-intake/DECISIONS.md` first (N4, N10, N11, N12, N13).

## What to change

1. **`crates/otaq-core/src/board/stage_choices.rs`, `judge`.** After the early `Ok` for `Chooser::User || by_user`,
   refuse any choice from `Chooser::ProjectAgent` with `Refusal::NotAllowed` and the text below, as a `pub const` (for
   example `NOYAN_USER_CHOICES_ONLY`) that tests and the skill can quote:

   > Noyan sets only the stage choices the user names: add --by-user, or leave the stage's own agent

   - It applies to `required` picks too.
   - It applies to `otaq create -f` files with `- **Stage:**` lines when the command has no `--by-user`. Batches are
     all or nothing, so the whole file is refused.
   - **Ticket agents (N12):** a `Chooser::TicketAgent` choice of `skip` without `by_user` is refused with
     `Refusal::NotAllowed` and N12's text, as a second `pub const`. Their agent picks and `required` stay allowed,
     under today's rules (reason, later stages only, never over the user's choice).
   - Update the module doc comment ("an agent (the Project agent, a ticket's own agent) only with a reason…") to say
     that the Project agent sets only the user's choices.
2. **`crates/otaq-core/src/prime.rs`, `render_choices`, `AgentReader::ProjectAgent`.** Replace the lead ("Pick another
   option for a ticket you create only when how hard the work is says so…") with one saying that a new ticket runs
   every stage's own agent, and that Noyan passes on only the choices the user names, with `--by-user`. Give an
   example: `otaq create "Title" --stage "<stage>=<pick>" --by-user --stdin`. In the same function, change the
   `AgentReader::TicketAgent` lead: it may pick another agent for a later stage with a reason, never `skip`. The
   example must never show `skip` for a ticket agent (today `pick` falls back to `skip` when a stage offers no other
   agent; leave such stages out of the example, or drop the example then).
3. **`crates/otaq-core/src/project_agents/prime.rs` (N13).** The opening line ("You are Noyan, the planning agent
   of this … project, in …, the user's own checkout. You investigate and file tickets…", ~line 100, and its expected
   text in the test at ~line 217) becomes N13's defining sentence, word for word, followed by where it runs ("You
   run in …, the user's own checkout; agents build tickets later in their own worktrees.").
4. **Tests.** Run the suite and fix what the change breaks. Known: `crates/otaq-core/tests/suite/planning_skips.rs`
   (Noyan's "ordinary work" skips: rewrite them as the user's via `--by-user`, or turn them into refusal tests),
   `stage_choices.rs`, `prime.rs`, and possibly `agent_reads.rs`, `human_checks.rs`, `flow_changes.rs`,
   `every_request.rs`, plus the CLI suite in `crates/otaq-cli/tests/suite/`.
   **Add:**
   - Noyan's own choice is refused with the exact text;
   - Noyan's `--by-user` choice is accepted and recorded as the user's;
   - a `-f` batch with a Stage line and no `--by-user` from Noyan creates nothing;
   - a ticket agent's later-stage agent pick still works;
  - a ticket agent's later-stage `skip` is refused with N12's exact text;
  - the user's skip, and an agent's `--by-user` skip, still work.

## Out of scope

- The skill texts (T2).
- UI labels for choices already stored (O3).

## Done when

- `scripts/check.sh lint test` passes.
- The refusal text appears exactly once in the code, as the constant.
