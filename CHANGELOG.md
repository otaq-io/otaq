# Changes

What changed for you in each release. The maintainer copies a version's section into its GitHub release notes
before Publish (`scripts/README.md`, "Making a release").

## Unreleased

- **Tickets have no External ID.** The field is gone from New ticket, Edit ticket, the ticket's details line, the
  PR dialog, `otaq show` and `prime`; History no longer lists changes to it. Your tickets keep everything else.
- **Your skills stay where they are.** Otaq no longer copies or imports skills. Each tool loads your skills the
  way it always does, and Otaq's own skills come with its plugin. A stage runs only on a tool that has its skill:
  otherwise its card waits in Needs you with "<tool> doesn't have the skill `/<name>`", **Check again** and
  **Change agent…**.
- **No fallback agents.** A tool at its limit, logged out, not installed, hidden or needing an update, or an agent
  whose model is no longer listed, makes its agents wait; the card says why. Settings → Agents' fallback per tool
  and the card's **Use fallback** are gone. Pick another agent for a ticket's stage with **Change agent…**.
- **A check stage has one agent.** A flow whose check listed several agents now reads as one check stage per agent
  ("Validate · Security reviewer"), in the same column. **Checks that ran side by side now run in turn**, one after
  the other, each with its own rounds. Flow files name a check's agent with `agent:`.
- **Any agent for any stage, for one ticket.** For a stage that hasn't started you can pick any agent whose tool has
  the stage's skill. Only you pick (or Noyan, passing on your words); a ticket's own agent no longer does.
- **Nothing is skipped on its own.** A check of yours is no longer skipped because the stages before it were; its
  choice "required" just means the check happens, as the flow says. **Skip this check** and **Move on anyway** show only on checks the flow marks skippable.
  A skipped or moved-past check no longer turns an automatic Delivery into a click: it delivers as the flow says,
  marked "Delivered without all checks".
- **A stage's skill can't be a tool's built-in command.** A flow whose stage names one, such as Claude Code's
  `/security-review`, is refused instead of applying and then holding the stage. Write a small skill that runs the
  command and name that skill.
