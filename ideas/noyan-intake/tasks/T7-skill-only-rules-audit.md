# T7. Audit: which app rules rely only on skill text? (N10)

**Depends on:** nothing. **Runs in parallel with:** T1–T5. **Read-only:** change no code and no skill.

Read `ideas/noyan-intake/DECISIONS.md` first (N10).

Users can replace any shipped skill (`plugin/skills/*`) with their own. Find the rules in the shipped skills that the
app **depends on** but that only the skill text enforces, so that a user's skill could break the board, its data or
its labels.

## How

1. For each shipped skill, list its "must" and "never" rules, and the commands it requires (for example "always finish
   with `otaq stage next`", "commit everything", "Plan shape `## Pieces`", "never push", "Backlog only", "long text
   through stdin").
2. For each rule, find whether the service or CLI enforces it (a refusal, a reminder, a parser) and cite the code. For
   example, `stage next` is refused while files are uncommitted, and Otaq types a reminder when a stage ends without
   `stage next`.
3. For each rule only the skill enforces, say what happens to the app if a custom skill drops it. Use one concrete
   scenario: "a custom Dev skill never calls `otaq stage next` → ticket stays in Dev; reminder typed; card shows …?".
   Verify by reading the code; run tests if needed.
4. Classify each one:
   - **app rule, needs code**: breaks the board, data or labels;
   - **behaviour, fine in a skill**: only loses a safety net;
   - **already enforced**.

## Output

`ideas/noyan-intake/T7-audit.md`: one table per skill (rule · enforced where · what breaks without it · class), then a
short list of proposed code guards, each worded as a candidate task. Don't implement any.
