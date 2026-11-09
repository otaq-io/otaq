# tuq

Product decisions (and, later, tech specs, milestone plans and orchestrator prompts) produced by the tuq skills
(`tuq:product-spec`, `tuq:step-back`, `tuq:tech-spec`, `tuq:milestone-plan`).

## Layout

- `decisions/` — one file per product decision: `NNN-short-summary.md`, numbered across the repository.
- `tech-specs/` — one tech spec per feature: `<feature>.md`, how it is built (the architecture), with the decisions
  it builds on in its header.
- `plans/` — one milestone plan per feature: `<feature>.md`, the pieces and what each waits for.
- `prompts/` — the orchestrator prompt for each plan: `<feature>-orchestrator.md`, which the agent that builds the
  work follows.

## Rules

- Each new decision takes the highest number here plus one; a clash between parallel branches is fixed at merge by
  renumbering the later branch's files and their references. Numbers are never reused.
- Before a decision is saved it is checked against the existing ones; when the user proceeds despite a conflict, the
  earlier one is marked `Superseded by NNN` (or `Partly superseded by NNN`), not rewritten.
- The index below is generated from the decision files (`tuq index`): don't edit it by hand.

## Decision index

<!-- tuq:index:start -->

| #   | Decision                                                                                                           | Status   | Summary                                                                                                                                                                                                                             |
| --- | ------------------------------------------------------------------------------------------------------------------ | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 001 | [A comment can carry several attachments, images among them](decisions/001-several-attachments-per-comment.md)     | Accepted | While writing a ticket comment, the user can add several attachments to it: images (pasted or attached) and text files or long pasted text, several of each. They are posted together with the comment's text.                      |
| 002 | [Any kind of file can be attached to a comment](decisions/002-any-file-type-can-be-attached.md)                    | Accepted | A comment's attachments can be files of any type. Images and text get a preview in the app; any other file shows as a plain chip with its name and size.                                                                            |
| 003 | [Attachments are added by paste, drag and drop, or an Attach button](decisions/003-paste-drop-or-attach-button.md) | Accepted | While writing a comment, the user adds attachments in any of three ways: pasting (⌘V), dropping files onto the comment box, or an Attach button that opens the system file picker. Each added file becomes its own chip in the box. |
| 004 | [Posted images show as thumbnails that open full size](decisions/004-image-thumbnails-click-to-enlarge.md)         | Accepted | In the ticket's comment thread, a posted comment's images show as a row of small thumbnails under its text; clicking one opens the image full size in the app.                                                                      |
| 005 | [Each attachment can be up to 20 MB, with no limit on how many](decisions/005-twenty-mb-per-file.md)               | Accepted | Every attachment, of any type, can be at most 20 MB (today's report limit); a comment can carry any number of them.                                                                                                                 |
| 006 | [Agents attach files under the same rules as the user](decisions/006-agents-attach-like-users.md)                  | Accepted | An agent's comment can carry several attachments of any type, like the user's, and its images show as thumbnails in the thread the same way.                                                                                        |
| 007 | [A comment can be sent with attachments and no text](decisions/007-attachments-alone-can-be-sent.md)               | Accepted | A comment needs either a line of text or at least one attachment; a pasted screenshot can be sent on its own. This replaces today's rule that a comment with only a "Pasted text" chip can't be sent.                               |
| 008 | [Editing a posted comment can add and remove its attachments](decisions/008-edit-attachments-of-posted-comment.md) | Accepted | When the user edits a posted comment, they can add attachments to it and remove existing ones, not only change its text. History keeps what was removed.                                                                            |

<!-- tuq:index:end -->
