# T7. Audit: which app rules rely only on skill text? (N10)

Audited on 2026-11-04 against this worktree's base (`noyan-intake-T7`, from `31c6b06a`). Read-only: no code or skill
changed. Each rule below comes from a shipped skill in `plugin/skills/*/SKILL.md`; "enforced where" cites the code
that refuses, reminds or parses, read in this worktree. I re-ran
`cargo nextest run -p otaq-core -E 'test(/^board::stage_choices::/) | test(/^policy::/) | test(/^domain::plan::/)'`
(20 passed) to confirm the choice rule, the policy table and the Plan parse as described.

**Classes**

- **already enforced**: the service or CLI refuses, reminds or recovers on its own.
- **behaviour, fine in a skill**: a custom skill that drops it loses a safety net; the board, its data and its labels
  stay right.
- **app rule, needs code**: a custom skill that drops it leaves the board, its data or its labels wrong.
- **T1–T5**: another task is changing this rule right now; no extra guard is proposed here.

**Two facts behind many rows**

- Agents run with the user's own tool permissions. Otaq pre-approves only `Bash(otaq:*)` and adds no permission mode
  (`crates/otaq-core/src/agent_tools/claude/launch.rs:16-17`). So Otaq can't stop `git push`, `git merge`, file edits or
  commits. It controls only the `otaq` commands, which the service judges by the caller's session
  (`crates/otaq-core/src/caller.rs:3-6`, `crates/otaq-core/src/policy.rs:194-344`).
- Every "never push / never merge / never edit" rule is therefore skill text, apart from the after-the-fact checks
  cited per row.

## Rules every ticket-stage skill shares

These rules appear in `work-ticket`, `address-review`, `restack`, `product-spec`, `tech-spec` and `milestone-plan`.

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Run `otaq prime` first | `stage next`/`stage done` refuse while comments are unread: `crates/otaq-core/src/service/board_requests.rs:791,824` → `crates/otaq-core/src/board/primed.rs:111` | — | already enforced |
| Always end with `otaq stage next` (or `stage done`) | Turn end without it: one reminder typed (`crates/otaq-core/src/signals/reminder.rs:1-10`, text at `:24-31`), then the card shows Idle. The user can still Move to next stage (`crates/otaq-core/src/board/review.rs:145`) | A custom Dev skill that never calls it: the ticket stays in Dev, the reminder is typed once, the card shows Idle and appears in Needs you; nothing is lost | already enforced |
| Commit everything; nothing uncommitted | `stage next` refused: `crates/otaq-core/src/board/review.rs:165-187` ("2 files are not committed…") | — | already enforced |
| Long output in `$OTAQ_RESOURCE_DIR`, never in the worktree | An untracked file there makes `stage next` refuse (same check) | — | already enforced |
| Comment at most 2,000 characters | `crates/otaq-core/src/board/ticket_comments.rs:77-95` | — | already enforced |
| A short comment on `stage next` saying what was done | Empty text is allowed and posts no comment (`crates/otaq-core/src/board/hand_over.rs:99-101`) | The next stage and the user get no hand-over; the board is still right | behaviour, fine in a skill |
| Long text through stdin, never quoted arguments | The CLI takes the summary, verdict and question only on stdin (`crates/otaq-cli/src/cli.rs:369-400`) | — | already enforced |
| `stage done` only with nothing committed | Refused once the branch has commits, and outside an Agent stage: `crates/otaq-core/src/review/close.rs:75-107` | — | already enforced |
| Stop the Run you started; never stop the user's | The user's Run can't be stopped by an agent (`crates/otaq-core/src/terminals/run.rs:780`, text `:1415`); an agent's Run stops with its stage (`crates/otaq-core/src/terminals/run.rs:794-814`, called at `crates/otaq-core/src/service/board_requests.rs:815`) | — | already enforced |
| Never start your own copy of the app on a port you pick | Not enforceable (a plain shell command) | The board never sees that app; it may take another ticket's port | behaviour, fine in a skill |
| Never push, never open a PR, never merge | Not enforceable (git runs under the user's permissions) | An early push leaves a remote branch nobody reviewed; Open PR later renames the branch and pushes the reviewed one (`crates/otaq-core/src/pr/flow.rs:194-207` counts pushed commits only once a PR exists). The board stays right **unless** the agent also runs `otaq pr opened`: see G2 | behaviour (safety); the board part is G2 |
| Questions: 2–4 options, labels ≤ 40 characters, one note per option | `crates/otaq-core/src/board/questions.rs:88-115` | — | already enforced |
| One open question at a time | The ticket stores one; a second replaces it (code behaviour) | — | already enforced |
| Commit with the commit skill, "<KEY>: …" messages, never `--no-verify`, never commit secrets | Not enforced | Commit history style only; secrets are the user's hooks' job | behaviour, fine in a skill |

## work-ticket (Dev, and helpers)

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Gap check; no code while a gap question is open | Not enforced | A wrong build, caught at review | behaviour, fine in a skill |
| Keep to the ticket; file the rest with `otaq create` | Agents may create Backlog tickets (`crates/otaq-core/src/policy.rs:293`) | — | behaviour, fine in a skill |
| Prove each criterion; reproduce a bug first | Not enforced | Weaker evidence; the Validate check is the net | behaviour, fine in a skill |
| `--manual-check` lines; each `stage next` replaces the stage's unticked ones | Replace rule in code (`crates/otaq-core/src/board/hand_over.rs:124-156`) | — | already enforced |
| Record product choices with `otaq decision record` | Not enforced | Fewer decisions shown at review | behaviour, fine in a skill |
| **With a Plan of pieces: start a helper per piece; finish the stage only once every piece is merged** | **Not enforced.** `stage next` checks only uncommitted files (`crates/otaq-core/src/board/review.rs:165-187`); nothing in the submit path looks at helpers (`crates/otaq-core/src/service/board_requests.rs:790-822`). Helpers stop only on Park, a drag back and Done (`crates/otaq-core/src/service/mod.rs:1074-1102`). The reminder is off while a helper isn't merged (`crates/otaq-core/src/signals/reminder.rs:8-9`) | A custom Dev skill that runs `stage next` with piece 3 still working: the ticket moves to Validate without piece 3's code, helper 3 keeps running and still hands back to the Dev session (`crates/otaq-core/src/helpers/mod.rs:544-560`). If that session merges it, the commit appears mid-check and the check is cancelled or discarded (`crates/otaq-core/src/validation/mod.rs:12-16`). The card shows piece rows "working" on a ticket in Validate, where the labels assume "4 pieces merged · moving to Validate" (`crates/otaq-view/src/view_models/pieces.rs:10-15`) | **app rule, needs code: G1** (T4/T5 lean on it for N8/N9) |
| Merge pieces only with `otaq helper merge`, never by hand | Not enforced (a manual `git merge` works) | Helper stays "being reviewed", its worktree isn't removed, and the Files tab's piece tags are missing (`crates/otaq-core/src/helpers/mod.rs:16-21`) | app rule; G1 catches it, because the helper isn't merged |
| Never merge a removed piece | `helper merge` doesn't check the Plan (any number may start, `crates/otaq-core/src/helpers/start.rs:35-90`) | Extra code on the branch, caught at review | behaviour, fine in a skill (see open question 2) |
| Only the agent in charge starts, merges and answers helpers | `crates/otaq-core/src/helpers/mod.rs:349-369`; policy `crates/otaq-core/src/policy.rs:331-336` | — | already enforced |
| Helpers stay one level deep | A helper may only read and hand back (`crates/otaq-core/src/policy.rs:369-381`) | — | already enforced |
| Helper: commit everything before `helper done` | Refused with uncommitted files (`crates/otaq-core/src/helpers/mod.rs:512-523`) | — | already enforced |
| Helper: never move the ticket, never run the app | `crates/otaq-core/src/policy.rs:369-381` (`HELPER_NO_RUN`, `HELPER_ONLY_ITS_PIECE`) | — | already enforced |
| Helper: build only its piece; never push or merge | Not enforced | Scope creep, caught by the agent in charge's `helper diff` | behaviour, fine in a skill |
| Helper: questions go to the agent in charge | `ask human` from a helper is routed to the agent in charge (`crates/otaq-core/src/service/permissions.rs:313-316`) | — | already enforced |
| A later stage's choice only with a reason; never over the user's choice | `crates/otaq-core/src/board/stage_choices.rs:436-476` | — | already enforced; **N12's "never skip" is T1** |
| (N8) Decide up front whether to split; re-split mid-way | Not in today's skill | — | **T5** (skill) / **T4** (whether Otaq allows it) |

## address-review

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Read the line comments (`otaq review feedback`) | Not enforced; `prime` is enforced (above) | Comments go unaddressed; the Validate check and the user's next review are the net | behaviour, fine in a skill |
| A big fix on a ticket with pieces goes to a new helper | Not enforced | — | behaviour, fine in a skill (G1 covers unmerged helpers) |

## restack

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Rebase onto the target `prime` names; resolve every conflict | `stage next` is allowed from Merge conflict fixing (`crates/otaq-core/src/board/review.rs:128`); Local merge rebases again itself, and a conflict brings the card back | The next Local merge finds the conflict again | behaviour, fine in a skill |
| Never stash, never fetch-and-reset | Not enforced | A stash hides changes from the "uncommitted" check, so work can be silently left out | behaviour, fine in a skill |

## product-spec / tech-spec

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Settle the product (or design) only; don't build the feature | Not enforced | Code lands early; Spec review is the net | behaviour, fine in a skill |
| Write decisions where the project keeps them; record the place in the description | Own-ticket edits allowed (`crates/otaq-core/src/policy.rs:294-301`) | — | behaviour, fine in a skill |
| Ask one question at a time | One open question per ticket (code) | — | already enforced |

## milestone-plan

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| **The Plan's fixed shape: `## Pieces` last, "N. Title — waits for …", only smaller numbers** ("Otaq reads it to start the helpers") | **Read leniently, never refused**: a Plan that doesn't parse has no pieces and no error (`crates/otaq-core/src/domain/plan.rs:14-18,42`); unknown waits are ignored and cycles come last (`crates/otaq-core/src/domain/plan.rs:94-125`). `otaq update --plan-file` replies only "Updated …" (`crates/otaq-cli/src/write.rs:236-282`). Note: `helper start` itself never reads the Plan (`crates/otaq-core/src/helpers/start.rs:35-90`); the Dev agent learns the pieces from `prime`'s "Pieces: 4 (1 → 2, 3 → 4)" line (`crates/otaq-core/src/prime.rs:1137-1146`) | A custom planning skill writes "### Steps" with bullets and no "Pieces" label: the card shows no "Plan · n pieces" (`crates/otaq-view/src/view_models/plan.rs:127-139`) and `prime` shows no Pieces line. The Dev agent builds alone, and nobody is told why | **app rule, needs code: G3** (labels and prime depend on the parse) |
| Never write statuses into the Plan | Statuses are Otaq's own records (`crates/otaq-view/src/view_models/pieces.rs:1-4`) | Stray words in the text only | behaviour, fine in a skill |
| File nothing (several tickets are Noyan's) | Any ticket agent may create Backlog tickets (`crates/otaq-core/src/policy.rs:293`) | Extra Backlog tickets "created by SHOP-40's agent", which the user sees and can delete | behaviour, fine in a skill |
| Piece size "about a day" | Not enforced | — | **T3** (N6's measure) |
| Write the plan into the project's docs and commit | Uncommitted refused (above) | — | behaviour, fine in a skill |

## validate (agent checks)

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Read-only judge: change nothing, never commit | Changed tracked files or HEAD at verdict time → verdict discarded (`crates/otaq-core/src/validation/mod.rs:12-16`) | — | already enforced (after the fact) |
| Only `check pass|fail`; no `stage next`, no tickets, no comments outside the verdict | `crates/otaq-core/src/policy.rs:221-256` ("…check can only give its verdict") | — | already enforced |
| Always give a verdict | Ending without one shows "ended without a verdict", with Retry (`crates/otaq-core/src/validation/mod.rs:34-37`) | — | already enforced |
| `prime --validate` first | Verdict refused while comments are unread (`crates/otaq-core/src/service/board_requests.rs:864`) | — | already enforced |
| Don't run `otaq run set` (it saves a setting) | A check may run its own ticket's `run` commands (`crates/otaq-core/src/policy.rs:239`) | A check could change the project's run command | behaviour, fine in a skill |
| Prove each finding; notes vs failures; `Checked:` lines | Not enforced | Weaker verdicts | behaviour, fine in a skill |

## open-pr / push (Delivery)

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Propose first, with every listed commit, in order | `crates/otaq-core/src/pr/flow.rs:357-401`; refused without an Open PR under way or once it is pushing (`crates/otaq-core/src/pr/commands.rs:15-49`) | — | already enforced |
| Never reword, rebase or rename yourself | Otaq rewords and renames itself on confirm (`crates/otaq-core/src/pr/flow.rs:406`); after `pr opened` the pushed tree is compared with what was reviewed (`crates/otaq-core/src/pr/verify.rs:1-3`, `crates/otaq-core/src/pr/service.rs:579-593`) | — | already enforced (warning) |
| Stop on a remote that isn't GitHub or Azure DevOps | Open PR is refused by the service first (`crates/otaq-core/src/pr/service.rs:215,376`) | — | already enforced |
| **Push only after Otaq says the proposal is confirmed; then `otaq pr opened <url>`** | **`pr opened` is accepted from any session of the ticket's own agent, at any state.** It is judged as "change own ticket" (`crates/otaq-core/src/service/permissions.rs:340`), and neither `Prs::opened` nor `Board::pr_opened` looks at the PR flow's state or the ticket's stage (`crates/otaq-core/src/pr/service.rs:557-595`, `crates/otaq-core/src/pr/commands.rs:80-118`) | (a) A custom Open PR skill pushes in turn 1 and reports at once: the user's dialog edits are never applied, and the flow just ends. (b) A custom Dev skill runs `gh pr create` and `otaq pr opened` while still in Dev: the ticket gets `pr_number` and `branch_pushed` (the branch name locks), and from then on Otaq treats it as having a PR. The user's later Open PR becomes a Push (`crates/otaq-core/src/pr/service.rs:208,367`), with no title or description proposal | **app rule, needs code: G2** |
| Never `--force`; push only the confirmed branch | Not enforceable; the content check above warns when the pushed tree differs | — | behaviour (safety) |
| Report the PR (`pr opened`) after pushing | Not enforced; the flow stays "Pushing SHOP-18…" until the user's Retry (`crates/otaq-core/src/pr/service.rs:478`) | The dialog waits; the user retries | behaviour, fine in a skill |

## answer-question

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Change nothing | A worktree with changed files is kept and listed in Cleanup (`crates/otaq-core/src/messages/answering.rs:1-8,346`) | — | already enforced (after the fact) |
| Reply with `otaq answer <ID>` | Not enforced. The ticket is Done, so its signals don't move (`crates/otaq-core/src/signals/mod.rs:52-60`), and the stage reminder doesn't cover it (`crates/otaq-core/src/signals/reminder.rs:9-10`) | A custom answer skill ends its turn without answering: the question stays routed "resumed" and the answer session stays open. It holds the ticket's one answer slot, so later questions to that ticket wait too (`crates/otaq-core/src/messages/answering.rs:126-132`). The asking agent waits with no sign of why. I didn't verify what the UI shows for the asker | app rule (small): G4 |
| Don't run `stage next`, don't file tickets | `stage next` is refused on a Done ticket (`crates/otaq-core/src/board/review.rs:128-137`); creating tickets is allowed (`crates/otaq-core/src/policy.rs:293`) | Stray Backlog tickets | behaviour, fine in a skill |

## noyan (Project agent)

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Only Backlog tickets: create, and edit/link/prioritise unstarted ones; never move, start, merge, close or delete | `crates/otaq-core/src/policy.rs:265-284` ("Noyan can only change Backlog tickets") | — | already enforced |
| Never comment | `crates/otaq-core/src/policy.rs:277` | — | already enforced |
| Name the conversation once; keep the user's name | `crates/otaq-core/src/project_agents/mod.rs:720-735` | — | already enforced |
| Never edit files, never commit, never push (it runs in the user's checkout) | Not enforced. Skill copies are git-ignored (`crates/otaq-core/src/project_agents/checkout.rs:1-7`); nothing watches the checkout | Edits left uncommitted in the checkout make every Local merge refuse ("shop-web has uncommitted changes on main. Clean up and retry.", `crates/otaq-core/src/git/merge.rs:444`). That is a clear refusal, but it names no culprit. A Noyan commit lands on the user's branch unreviewed | behaviour (safety); see "Considered, not proposed" |
| Read a ticket's terminal only in its project | `crates/otaq-core/src/policy.rs:268-272` | — | already enforced |
| Propose before creating two or more tickets | Not enforced | Tickets appear in Backlog without the user's go-ahead; the user can delete them | behaviour, fine in a skill; **T2** keeps it (N2) |
| **Stage choices: only with `--reason`; never over the user's choice; `--by-user` only for choices the user named** | Reason and the user's choice are enforced (`crates/otaq-core/src/board/stage_choices.rs:457-465`). `--by-user` is taken on trust from any agent | Noyan's own skips read as its own today. Any `--by-user` from an agent reads "set by you" | **T1** (refuses Noyan's own choices, N11) and **T2** (skill); see open question 1 for `--by-user` |
| "Ordinary work skips planning"; "a cheaper agent for a minor fix" | Allowed by the service today | — | **T1** (refusal) and **T2** (skill text, N4); **T8** clears stored ones (N14) |
| Decide the shape: one ticket with a Plan, or several; write the Plan | `--plan` from Noyan is accepted on Backlog tickets (edit rule above) | — | **T2** (N1: Noyan writes no Plan); see open question 3 |
| Imports use the user's skills | Not enforced | — | behaviour, fine in a skill |

## flow (Noyan changing the flow)

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Never change the live flow; only propose | Apply, Cancel, the draft and the editor's buttons are the user's (`crates/otaq-core/src/service/permissions.rs:374-382`) | — | already enforced |
| Propose for its own project only | `crates/otaq-core/src/policy.rs:206-217` | — | already enforced |
| The rules of a flow (one Delivery last, `fail_to`, `skip` leaving a stage that runs…) | `otaq flow propose` refuses, naming the stage (the skill's own list, checked in `crates/otaq-core/src/flow/check.rs`) | — | already enforced |
| A whole file every time | A partial file is a proposal that drops stages; the editor marks each change and nothing applies before the user's Apply | — | behaviour, fine in a skill |
| Write a stage's skill only in the folder Otaq gives | `--done` checks and copies that folder only (`crates/otaq-core/src/flow/write_skill.rs:5-12`) | Files written elsewhere are just stray edits (as for Noyan above) | already enforced for what Otaq takes in |
| `why:` lines with real sources; no `skip` or automatic Delivery unless asked | Otaq doesn't act on `why`; the editor warns about an automatic Delivery with no check | — | behaviour, fine in a skill |
| First flow: read only your own tool's history and this repo | Not enforceable | Privacy, not board state | behaviour, fine in a skill |

## commit

| Rule | Enforced where | What breaks without it | Class |
|---|---|---|---|
| Everything committed, `git status` clean | `stage next` / `helper done` refuse otherwise (above) | — | already enforced |
| "<KEY>: …" messages; never `--no-verify`; no secrets; never push | Not enforced | Style and safety only | behaviour, fine in a skill |

## Proposed code guards (candidate tasks)

Each guard is worded as a task. None is implemented here.

**G1. `otaq stage next` waits for every started helper to be merged.**
Refuse an agent's `stage next` while its ticket has a helper that was started and isn't merged ("working", "held",
"sent back" or "being reviewed"). Use a clear refusal that names them, for example
"40.3 and 40.4 aren't merged yet: merge them with `otaq helper merge <n>`, or send them back". Where: the
`Request::TicketStageNext` handler (`crates/otaq-core/src/service/board_requests.rs:790`), before the hand-over.
Read the helpers from `crates/otaq-core/src/helpers/store.rs`. Leave the user's Move to next stage alone, and leave
`stage done` alone too, since it already refuses once there are commits. This also catches pieces merged by hand with
`git merge` instead of `otaq helper merge`. It matters more once N8 and N9 make mid-build splitting the norm
(T4, T5). It needs open question 2 settled first.

**G2. `otaq pr opened` only after Otaq has confirmed the push.**
Accept `pr opened` from an agent only while the ticket's PR flow is "pushing"
(`crates/otaq-core/src/pr/flow.rs:26-33`). Refuse it otherwise, naming the step, for example "SHOP-18's pull request
isn't confirmed yet: propose it with `otaq pr propose --stdin` and wait until Otaq says to push". Where: `Prs::opened`
(`crates/otaq-core/src/pr/service.rs:557`). The user's own Mark PR requests stay as they are. This keeps `pr_number`,
`branch_pushed` and the card's "PR open" true to what the user confirmed. Check that the "stray session end" case at
`crates/otaq-core/src/pr/service.rs:565` still has a flow under way when it reports.

**G3. Say what Otaq read from a Plan when an agent writes one.**
On `otaq create|update --plan*`, reply with what the parse found: "Plan saved · Pieces: 4 (1 → 2, 3 → 4)". When the
text names a Pieces list but no item reads, or a piece waits for one that doesn't exist or comes later, warn instead:
"Plan saved, but no pieces were read: number each piece under `## Pieces` (`1. Title — waits for nothing`)". Keep this
a warning, not a refusal: the parse is lenient on purpose because the user edits the Plan too
(`crates/otaq-core/src/domain/plan.rs:14-18`). Where: the `TicketUpdate`/`TicketCreate` replies and their CLI output
(`crates/otaq-cli/src/write.rs:270-280`). T3's and T5's skill text can then rely on the read-back, which matters for N8
(the Dev agent writing its own Plan, T4).

**G4. Remind an answer session that ends its turn without `otaq answer`.**
When an answer-only session's turn ends with its question still open, type one reminder ("Answer with `otaq answer <id>
--stdin`, or say in the answer that you don't know"). If the next turn ends the same way, put the question in Needs
you, as when finished tickets don't answer (`crates/otaq-core/src/messages/answering.rs:134-138`). This is low
priority: answer sessions are rare and the skill is short.

### Considered, not proposed

- **Noyan editing or committing in the user's checkout.** A turn-end check that compares the checkout's `git status`
  and HEAD with how they were at the session's start could show a notice. I left it out: Local merge already refuses
  clearly, and N10 puts the board's needs first. It's the maintainer's call (it would be the first time Otaq watches
  the user's checkout).
- **`git push` / `git merge` by ticket agents.** These are out of the service's reach by design: the user's tool
  permissions apply (d0011, `crates/otaq-core/src/agent_tools/claude/launch.rs:16-17`). G2 covers the only part the
  board depends on.
- **A ticket agent filing tickets from Milestone plan.** It's allowed for every ticket agent, and the user sees the
  result in Backlog.

## Open questions (not decided here)

1. **May ticket agents use `--by-user`?** Today any agent's `--by-user` choice is stored as the user's
   (`crates/otaq-core/src/board/stage_choices.rs:188-191`). It also skips the reason, the "never over the user's
   choice" rule and the "later stages only" rule (`crates/otaq-core/src/board/stage_choices.rs:454-456`, test row
   "agent over the user's choice, in the user's words" at `:1175-1178`). T1 keeps "an agent's `--by-user` skip still
   works", so the guarantee "every skip on the board was the user's" (N12) rests on the agent being honest about what
   the user said. The board's "set by you" label depends on it, so by N10 it is an app rule. The options: allow
   `--by-user` from Noyan only (where the user talks to it), or keep it for every agent as the way to pass on what
   the user typed into a ticket's terminal. DECISIONS.md doesn't settle this.
2. **How does the agent in charge drop a piece?** No command stops or drops a helper. The skill says to tell a
   removed piece's helper to stop with `otaq helper back`, which leaves it "sent back" for good. With G1 that ticket
   could never end its stage. Should there be a drop (for example `otaq helper drop <n>`, the helper stopped and its
   branch kept until Done)? Or should `stage next` stop unmerged helpers instead of refusing? This matters for N9's
   re-split too, where the agent in charge replaces pieces.
3. **Should the service refuse a Plan from Noyan (N1)?** It accepts `--plan` on Backlog tickets today. The board
   doesn't depend on who wrote the Plan ("Written by Noyan" is a working label), so by N10 this is skill-only (T2).
   Say so if N1 should be enforced anyway.
