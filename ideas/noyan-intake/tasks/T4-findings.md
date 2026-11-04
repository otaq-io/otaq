# T4 findings: the Plan written and grown during the build stage

Input for T5 (the `work-ticket` skill). Proved by `crates/otaq-core/tests/suite/plan_mid_build.rs` (requests sent
through the socket from inside the agent's own session, so they are judged as that agent, as the CLI's are).

**Result: Otaq already allows both cases. No service change was needed.** Every command below is accepted for the
ticket's own agent in its build stage, and nothing else changed for other actors (a ticket's agent still can't edit
another started ticket).

Display names: piece 3 of SHOP-40 is "40.3". In the tests the ticket is SHOP-1, so its pieces read "1.1", "1.3".

## Case A: the Dev agent writes a Plan on its own started ticket

The ticket arrived with no Plan (`otaq prime` has no "## Plan" section). After the gap check, before coding, the agent
decides by the size measure to split.

### Working sequence

1. Write the Plan to a file in the shape the skills already use (a "Pieces" heading, numbered items, "waits for"
   after an em dash, optional indented text under an item):

   ```markdown
   Why: the API lands first so the page has something to call.

   ## Pieces

   1. Wishlist API — waits for nothing
   2. Wishlist button — waits for 1
   3. Wishlist page — waits for 1
   ```

2. `otaq update --plan-file plan.md` (the key defaults to `$OTAQ_TICKET`; `otaq update $OTAQ_TICKET --plan-file …`
   works the same; `--plan-stdin` too). Printed: `Updated SHOP-40 · Wishlist`.
   - Accepted: the policy lets a ticket's agent edit its own ticket whatever its status.
   - The Plan is recorded as written by the stage: `plan_by` is `stage:Dev` (the sheet reads "Written by Dev").
     History keeps the version, and the card gets the usual "Updated by agent" marker, as any agent edit does.
   - **No message comes back to the agent** for its own edit (change notes are for the user's edits only).
3. `otaq prime` now shows the Plan and its order line:

   ```text
   ## Plan

   Why: the API lands first so the page has something to call.

   ## Pieces

   1. Wishlist API — waits for nothing
   2. Wishlist button — waits for 1
   3. Wishlist page — waits for 1

   Pieces: 3 (1 → 2, 3)
   ```

4. `otaq helper start 1 --title "Wishlist API"` (optionally `--stdin` with a note). Reply:

   ```text
   Started 40.1's helper (Wishlist API) on otaq/SHOP-40.1, in <worktree path>. It hands its piece back with `otaq helper done`; read its work with `otaq helper diff 1`, send it back with `otaq helper back 1 --stdin`, merge it with `otaq helper merge 1`.
   ```

   The helper's `otaq prime` starts with its piece, read from the Plan: `## Your piece` / `40.1 Wishlist API — waits
   for nothing`.

5. From here the agent is the agent in charge, exactly as on a ticket that arrived with pieces ("With a Plan: work as
   the agent in charge" in the skill).

### Refusal kept

Writing the Plan of another started ticket (one it didn't create) is refused as before:

```text
SHOP-2 was created by you; SHOP-1's agent can only change tickets it created
```

("you" is the user from the agent's side; for a ticket another agent created it names that agent, for Noyan's
"Noyan". For a ticket it created that has started: "SHOP-2 has started; SHOP-1's agent can only change tickets it
created that haven't started".)

## Case B: the agent in charge grows the Plan while helpers run

Typical run (helper re-split): helpers 1 and 3 are working; helper 1 finds its piece bigger, commits what it built and
hands back with a suggested split.

### What the agent in charge sees, and the working sequence

1. The helper runs `otaq helper done --stdin` with, for example:

   ```text
   Add and list are done; export is left.

   Suggest: 4 export API, 5 export button.
   ```

   The agent in charge is told, typed into its session when it is idle:

   ```text
   40.1 handed its piece back: Add and list are done; export is left. Read it with `otaq helper diff 1`, then merge it (`otaq helper merge 1`) or send it back (`otaq helper back 1 --stdin`).
   ```

   **Only the summary's first line is in that message.** The whole text (with the suggested split) is a ticket
   comment by "Developer · 40.1", shown under "## Comments" in the agent in charge's `otaq prime`.

2. Review and merge what was built: `otaq helper diff 1`, then `otaq helper merge 1` (squashed onto the ticket's
   branch; the helper's session ends and its worktree goes).

3. Rewrite the Plan with `otaq update --plan-file plan.md`: piece 1's text shrunk to what was built, new pieces added
   with **new whole numbers**, a running piece's text edited if needed:

   ```markdown
   ## Pieces

   1. Wishlist API — waits for nothing
      Built: add and list.
   2. Wishlist button — waits for 1
   3. Wishlist page — waits for 1
      Show the list only; export comes with 5.
   4. Export API — waits for 1
   5. Export button — waits for 3, 4
   ```

   Accepted, with History keeping both versions and `plan_by` still `stage:Dev`. `otaq prime` shows
   `Pieces: 5 (1 → 2, 3, 4 → 5)`.

4. Start the new pieces that can start: `otaq helper start 4 --title "Export API"` → `Started 40.4's helper (Export
   API) on otaq/SHOP-40.4, in …`. Its prime shows `40.4 Export API — waits for 40.1`.

### Answers to the task's checks

- **Is the update accepted?** Yes, with helpers running.
- **Does "The plan changed: …" reach the session?** Not for the agent's own edit: it sends **no note to anyone**,
  neither to the agent in charge nor to any helper. Only a **user's** Plan edit makes a note, and it goes to the agent
  in charge only, never to helpers, for example:
  `The ticket was updated by the user. The plan changed: 40.5 removed. Run `otaq prime` for the full ticket.`
  (pieces are named "added", "removed", "edited"; "The plan changed." when no piece changed; "The plan was cleared.")
- **Can new pieces start?** Yes. `helper start` doesn't check the Plan at all; a helper's prime reads its piece from
  the current Plan.
- **Is a running piece left alone when only its text changes?** Yes: same session, still running, status still
  "working", nothing typed into it. Its next `otaq prime` shows the new text. It is not told that it changed.

## What the skill (T5) must warn about

1. **Number new pieces with the next free whole number** (5, 6), never "3a"/"3b". The Pieces list only reads
   `<number>. ` or `<number>) ` items: "3a. Export API" is not a piece, it is read as text under the piece above it.
   A helper's suggested split should use whole numbers too ("suggest 4 export API, 5 export button").
2. **Never renumber or reuse a number.** Helpers are kept per piece number. A merged piece can't start again (`40.1 is
   merged already`), and a working one can't start twice (`40.3's helper is already working: send it a note with
   `otaq helper back 3 --stdin``). The work left over from a merged piece goes into new pieces.
3. **Don't remove a piece whose helper still works**: the helper keeps running, and its prime then says
   "(the Plan doesn't list it: the agent in charge's notes say what it is)". Stop it with a note first.
4. **Changing the Plan tells no one.** After editing a running piece's text, tell its helper with
   `otaq helper back <n> --stdin` (this also marks it "sent back"). Its next `otaq prime` has the new text.
5. **Name the piece as the Plan does** in `helper start --title`: the title is stored with the helper as given; Otaq
   doesn't check it against the Plan.
6. **The hand-back message is one line.** A helper that hands back early must put "what's left" in the first line;
   the agent in charge reads the rest (the suggested split) under "## Comments" in `otaq prime`.
7. **Only the agent in charge changes the Plan.** A helper's own `otaq update` is refused with `a helper works on its
   piece; the agent in charge moves the ticket` (helpers may only read, hand back and ask; from the policy, not
   re-tested here). The skill's helper part should say to suggest a split in the hand-back instead.
8. **Commit before `helper done`**, as before: it is refused with uncommitted files (`1 file is not committed. Commit
   it, or discard it if it's a scratch file`). The early hand-back merges only what was committed.
9. Writing the Plan raises the card's "Updated by agent" marker and History shows "Written by Dev". That is expected;
   nothing to do.

## Noticed, outside this task

- The current `work-ticket` skill (line ~164) already tells the agent to start a helper "with the next free piece
  number, for example piece 5 when the Plan has 4" for a Validate fix, without writing it into the Plan. That works
  (the helper's prime then says the Plan doesn't list it); T5 may want to say to add it to the Plan first.
- `helper start` accepts any piece number and title, whether or not the Plan lists it. Not a problem for N8/N9, but
  it means a typo in the number starts a helper for a piece that doesn't exist.
