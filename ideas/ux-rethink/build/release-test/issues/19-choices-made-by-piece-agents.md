# 19 · Choices the piece agents made that the plan didn't spell out

**Kind:** to confirm, not known bugs · Read them with the maintainer and keep or change each.

- **p1:** a request that panics now answers with an error ("This is a bug in Otaq; please report it").
- **p2:** new amber "couldn't check" state with Re-check.
  - Claude is checked with `claude auth status --json`; an older Claude without that command reads "couldn't check".
  - Copilot is asked through `--acp`, which needs the network; offline it reads "couldn't check".
- **p3:** the tool-stop reason is sorted by keywords in the tool's message. Resume on a tool-error Idle types
  "continue". The card's action is Open.
- **p5:** the delivery session stops on any status move out of Review / To deliver / PR open. The Windows
  leftover-folder fix passed (no worktree left).
- **p6:** the unsent-text check reads the screen after keys settle (500 ms), and faded text right of the cursor counts as
  empty. The footer wording also changed during a command question (not asked for).
- **p8:** a held ticket counts as in its stage, so reorders and flow switches wait for it too. Hold rows are cleared
  only by Apply now moves, not by ordinary drags.
- **p9:** "Move on anyway" after no result stays for skippable checks only. Overview shows Move on anyway on
  out-of-rounds checks on the default flow too.
- **p10:** wording taken from RESULTS-D; canvas flow-01b not opened. Enter on a search with no match opens the words box
  (sends nothing).
- **p11:** the "drafting a flow" line stays during the agent's permission prompts and questions.
- **p15:** the own-agent line also shows a later stage's choice while the ticket is in an earlier one. Overview makes
  an extra board.agents call.
- **p16:** the free-agent wording applies only to WaitingForSlot.
- **p17:** the palette also says "Resume <name>" (was "Resume this project").
- **p18:** "<agent> wants your permission" when there is no command (not drawn).
- **p20:** canvas ticket-05b not seen. The "1 of 3" hint is the agent's own wording, and "returns to Review" is fixed
  text.
- **p21:** the comment reads "Dev asked to send … back to Product spec: <reason>", so the stage name repeats.
