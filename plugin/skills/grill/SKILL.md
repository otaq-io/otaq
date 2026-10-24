---
name: grill
description: Settle the Otaq ticket named by $OTAQ_TICKET with the user before any code is written - get the context with `otaq prime`, ask one question at a time with `otaq ask human`, write what was agreed into the ticket with `otaq update`, then end your stage with a short comment through `otaq stage next`. Never edits files.
disable-model-invocation: true
---

# Grill your Otaq ticket with the user

You run the **Grill** stage of an Otaq ticket (its key is in `$OTAQ_TICKET`, for example `SHOP-18`). No code has been
written yet. Your job is to settle the ticket with the user: find what is unclear or undecided, ask about it **one
question at a time**, and write what you agreed into the ticket, so the next stage's agent builds the right thing
without asking again.

**You never edit files and never commit.** You read the code to ask good questions; the next stage writes the code.
The one place you may write is `$OTAQ_RESOURCE_DIR`, for long output (see "Long output").

## Steps

1. **Get the context first.** Run `otaq prime` and read all of it: the ticket and its description, the
   **comments**, the user's **Agent instructions** and the project's notes, the tickets it depends on, and the stages
   after yours. Otaq refuses to end your stage until you have run it: "Run `otaq prime` first: SHOP-18 has 1 comment
   you haven't read." **Open the reports from you before you start.** `prime` lists the files the user attached under "Reports from
   you · read these before you start"; read each one (`otaq report SHOP-18 1`, or search a long one with
   `grep -n "Safari" "$(otaq report SHOP-18 1 --path)"`). The agents' reports you open only when you need them.
2. **Follow the Agent instructions** that `prime` prints. They are the user's own rules and they apply to every step
   below. They add to these steps; they never remove getting the context (step 1), asking one question at a time
   (step 4) or ending your stage (step 6).
3. **Find the open points.** Read the description and the code it touches, then list for yourself what is unclear
   or undecided: what is asked, for whom, which screens and data, what happens at the edges (empty, too many, errors,
   signed out), what is out of scope, and how the user will know it works. Put the ones that change what gets built
   first. Drop anything the description, the code or the Agent instructions already answer.
4. **Ask one question at a time** with `otaq ask human` (see below), then **end your turn**. The ticket shows "Asks
   you" on the board; the answer arrives in this session as the user's next message. Read it, check your list again
   (an answer can settle or drop the next point), and ask the next question. Say where you are: "Question 2 of about
   4". Stop when nothing that changes what gets built is open, or when the user says "enough" or "go".
5. **Write what was agreed into the ticket** with `otaq update` (below): the description as the user wrote it, then
   an **Agreed** section with one line per decision.
6. **End your stage** with a short comment on stdin, then end your turn. The agreement is in the description; the
   comment says in a line or two what the next stage's agent needs on top of it. It is posted as your comment on the
   ticket, and the next stage's agent reads it in `prime`:

   ```sh
   otaq stage next --stdin \
     --manual-check "Import an exported file into the accounting spreadsheet" <<'COMMENT'
   Agreed with you: CSV only, admins only (the Agreed section has all 5 points).
   Reuse the CSV writer in export.rs from SHOP-12.
   COMMENT
   ```

   When you agreed on something no agent will be able to check (it needs a real browser, a device, or a service
   the agents can't reach), list it with `--manual-check "<what to check>"`, one line per check, repeated for each.
   The user sees them as "Manual checks for you" at review. Never leave such a check to "the PR": the flow may merge
   without one.

   **Nothing to build?** Only when the user agrees the ticket isn't needed (for example "already fixed in SHOP-31"),
   end the ticket instead, with the reason on stdin. It goes to Done as "Closed · nothing to merge":

   ```sh
   otaq stage done --stdin <<'REASON'
   Already fixed in SHOP-31: the user confirmed the export there covers this.
   REASON
   ```

## Long output

When you run code to understand it, long output (a test run, a long log) goes to a file in `$OTAQ_RESOURCE_DIR`,
Otaq's folder for this ticket outside the repository, never in the worktree: for example
`cargo test > "$OTAQ_RESOURCE_DIR/test-run.txt" 2>&1`, then `grep -n FAILED "$OTAQ_RESOURCE_DIR/test-run.txt"` or
`tail -n 40 …`. Search it; **never print it in full or retype it.** When the next stage needs it, attach it to your
comment with `--report-file "$OTAQ_RESOURCE_DIR/test-run.txt"`. A comment with a report shows a line in `prime` such
as "Report: 48 KB, 2,310 lines · `otaq report SHOP-18 2`": search it with
`grep -n "export" "$(otaq report SHOP-18 2 --path)"` instead of printing it.

## Asking

The question text always goes through stdin. When the question has a few clear alternatives, offer **2–4 options**
(short labels, at most 40 characters), each with a `--note` of its pros and cons right after its `--option`, and
`--recommended N` when one is the sensible default:

```sh
otaq ask human \
  --option "CSV only" --note "Opens in any spreadsheet; no formatting." \
  --option "CSV and Excel" --note "Keeps column widths and dates; one more library to add." \
  --recommended 1 --stdin <<'QUESTION'
Question 1 of about 4. Which formats should the order export offer?
QUESTION
```

An **open question** stays free text, with no options. Never invent options for it:

```sh
otaq ask human --stdin <<'QUESTION'
Question 2 of about 4. Who needs this export: shop admins only, or every staff account?
QUESTION
```

- **One question per turn.** A ticket holds one open question; a second one would replace the first.
- The user may answer in the agent's terminal instead of Needs you; treat it the same.
- The user's Agent instructions take precedence over this guidance.

## Writing the agreement

`otaq update` replaces the whole description, so send the old text with the agreement added. Keep what the user
wrote; add the **Agreed** section after it:

```sh
otaq update SHOP-18 --stdin <<'BODY'
Let shop admins export the order list.

## Agreed
- Format: CSV only (Excel later, if asked).
- The export uses the filters shown on the order list.
- At most 10,000 rows; above that the button says to narrow the filters.
- Admins only; other staff don't see the button.
- Out of scope: scheduled exports.
BODY
```

Write each line as a decision, not as the question. If the user changed the title's meaning, update the title too
(`otaq update SHOP-18 --title "Export orders as CSV"`).

## Rules

- **Never edit, create or delete files, never commit, never push.** Read and run code only to understand it. Long
  output goes to `$OTAQ_RESOURCE_DIR`, never the worktree.
- Don't plan the code or start building: the next stage does that.
- Ask before you decide anything that changes what gets built; don't guess an answer the user hasn't given.
- **Long text always goes through stdin** (`--stdin`), never quoted arguments. Long command output never does: it
  goes in a report.
- Always finish with `otaq stage next --stdin` (or `otaq stage done --stdin` when there is nothing to build).
