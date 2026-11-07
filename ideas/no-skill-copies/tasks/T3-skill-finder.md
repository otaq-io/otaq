# T3. Finding the skills the user already has, read-only (S6)

**Depends on:** nothing. **Runs in parallel with:** T1 and T2.

Read `ideas/no-skill-copies/DECISIONS.md` first (S6).

## What to change

- **A new core module**, `crates/otaq-core/src/skills/found.rs` or similar. Coordinate the file name with
  `skills/mod.rs` only by adding a `pub mod` line; T2 rewrites the rest of that file. Given a project's checkout:
  - list every skill in S6's folders: personal (home folder, `CODEX_HOME`) and project;
  - a skill is a folder holding `SKILL.md`;
  - its name is the front matter's `name:`, else the folder name. Reuse `skills::folder::skill_name`'s parser, or
    move it, without changing its rules;
  - its description is the front matter's `description:`, folded to one line;
  - for each, record where it lives (which folder, personal or project);
  - when one name is in several places, list it once, with every place.
- **Otaq's own skills** come from `agents::shipped` (name and description from `plugin/skills/<name>/SKILL.md` as
  embedded or found), marked as Otaq's.
- **Reading only.** Never write, never follow a link out of the skills folder, and never fail the whole list
  because one SKILL.md doesn't read: skip that one and log it at debug level. Missing folders are normal.
- **Home folder and `CODEX_HOME`:** use the same helpers the adapters already use (look at `codex::setup` for
  `CODEX_HOME`), so tests can point them at a temporary home.
- **A service request** (`skills.list`, with the project key) returning the list, plus its `protocol.rs` entry and
  `every_request.rs` coverage. Answer with plain data. No view model yet: T8 draws it.
- **A check** `Board::has_skill(project, name) -> bool`: true for Otaq's own names and for any found name. T6 uses
  it for S6's warning.

## Tests

- A temporary home and checkout with skills in several S6 folders. Cover:
  - a name in two places;
  - a broken SKILL.md, which is skipped;
  - a folder with no SKILL.md, which isn't a skill;
  - a link that leaves the folder, which isn't followed;
  - `CODEX_HOME` set;
  - Otaq's own skills listed and marked.
- `has_skill` is true for `validate` with no folders at all.
- Windows: nothing Unix-only, except link tests under `#[cfg(unix)]` with the usual comment.

## Done when

- `scripts/check.sh lint test` passes.
- Report the exact folder list you implemented, and any folder S6 names that a tool doesn't actually read, according
  to the adapters' own comments or the tools' docs. Don't add folders beyond S6 without asking.
