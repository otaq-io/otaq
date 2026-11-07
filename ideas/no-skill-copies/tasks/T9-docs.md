# T9. Docs (S1–S8)

**Depends on:** T1–T8, all merged.

Read `ideas/no-skill-copies/DECISIONS.md` first.

## What to change

- **`docs/how-it-works.md`, `docs/otaq-command.md`, `docs/install.md`, `docs/development.md`, `README.md`:**
  - remove every description of skill copies, `otaq-<name>` names, `.otaq-copy`, importing a skill folder, and an
    agent's skill list;
  - say, in the docs' own plain style:
    - your skills stay where they are and your agent tools find them;
    - Otaq's own skills come with its plugin;
    - a flow stage names the skill it runs;
    - "Ask the agent to write it" saves into the project for you to commit.
- **`plugin/*/plugin.json` descriptions:** check that T1's wording matches the docs.
- **Module comments:** scan `crates/` for remaining comments that describe copies, imports or "skills by one name"
  as current (`rg -i "copy of a skill|skills by one name|imported skill|otaq-<name>" crates`). Fix those that are
  stale, and leave historical changelog-style references.
- **`website/`:** search its sources, not `dist/` or `node_modules/`, for the same, and fix the text only. Don't
  rebuild `dist/`.

## Done when

- `scripts/check.sh` passes.
- `rg -i "import skill folder|otaq-copy|copies the skills" docs README.md website/src` finds nothing.
