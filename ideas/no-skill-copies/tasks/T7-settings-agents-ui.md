# T7. Settings → Agents without skills (S4)

**Depends on:** T6. **Runs in parallel with:** T8.

Read `ideas/no-skill-copies/DECISIONS.md` first (S4).

## What to change

- **View models**, `crates/otaq-view/src/view_models/agent_page.rs` and `agents.rs`: remove the Skills field, the
  skill rows, **Import skill folder…**, **Re-import**, the skill dialog, and any skills summary in the agents list.
  Remove the Draft operations that change skills.
- **Desktop API**, `crates/otaq-desktop/src/api/settings.rs`: remove the skill requests and fields. Undo any stopgap
  T6 made there.
- **Generated types:** regenerate with `cargo test -p otaq-desktop generated_types_are_fresh` (see
  `scripts/README.md`, step 3).
  - `AgentSkillDialog.ts`, `AgentSkillRow.ts` and `AgentSkillsField.ts` go;
  - `AgentPage.ts` loses its skills field.
- **React**, `crates/otaq-desktop/ui/src/screens/settings/Agents.tsx` and `agents.test.ts`: remove the skills
  section, the import dialog and the file picker call. Check the page still lays out well with the section gone, in
  light and dark.
- **Words:** no leftover copy mentions skills on the agent page, the agents list, or the move-over notice.

## Tests

- View-model tests: an agent page has no skills field.
- `agents.test.ts` is updated.
- `scripts/check.sh frontend` passes.

## Done when

- `scripts/check.sh` passes in full (frontend, lint, test, doctest).
- Report a short description of the agent page as it now reads, top to bottom. The orchestrator checks it by hand.
