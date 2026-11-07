# T8. Flow editor: pick from the user's skills, the warning, and the written skill (S5, S6, S7)

**Depends on:** T3 (`skills.list`) and T6 (the warning, the written state). **Runs in parallel with:** T7.

Read `ideas/no-skill-copies/DECISIONS.md` first (S5–S7).

## What to change

### Pick a skill…

`FlowEditorSkillPicker` (view model `crates/otaq-view/src/view_models/flow_editor.rs`, around 976 and the picker
builder; desktop `crates/otaq-desktop/src/api/flow_editor.rs:~320`):

- List every skill from `skills.list` for the flow's project: the user's skills first, then Otaq's own, labelled
  as Otaq's.
- Each item shows its name, its description as the second line (as today), and where it lives ("Personal", "This
  project"), in the form the picker's existing second line allows.
- Searchable, as today.
- **A name typed by hand** is allowed. If the picker has no free-text entry yet, add the smallest one: "Use
  `<typed text>`" as the last item while searching.
- With nothing found, Otaq's own skills still show, so the list is never empty. Remove the "no imported skills"
  line.
- Remove the desktop code that resolves `SkillRef::Imported`. T6 may have left a stopgap there.

### The warning (S6)

- A stage whose skill T6 marked unknown shows **"You don't have the skill `<name>`"**, as a warning in the stage's
  Skill field and wherever the editor lists problems.
- It doesn't block Apply. Use the existing warning style; don't invent one.

### The written skill (S7)

- `FlowEditorSkillState::written` shows **"Saved in `<folder>` in this project. Commit it so agents get it in
  their worktrees."**, with `<folder>` from T6.
- Keep Open, which opens that folder, and Undo, which follows T6's rule.
- Drop text about Otaq's copy.

### Proposals

Remove "Import its folder…" from the proposal review UI, now that T6 has removed `ImportSkillFolder`. The warning
replaces it.

### Generated types and React

- Regenerate the types.
- Update the flow editor's React parts and tests for the picker items, the free-text item, the warning and the
  written text.

## Tests

- View-model tests:
  - the picker lists a personal skill, a project skill and Otaq's own, in that order;
  - free text works;
  - the warning shows for an unknown name and not for `validate`;
  - the written state shows S7's text with the folder.
- Front-end tests are updated.
- `scripts/check.sh frontend` passes.

## Done when

- `scripts/check.sh` passes in full.
- Report what the Skill field shows in each state (set, empty with picker, writing, written, warning). The
  orchestrator checks it by hand.
