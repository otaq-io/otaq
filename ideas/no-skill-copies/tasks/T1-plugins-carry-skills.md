# T1. Otaq's own skills ship inside its per-tool plugins (S2)

**Depends on:** nothing. **Runs in parallel with:** T2 and T3.

Read `ideas/no-skill-copies/DECISIONS.md` first (S2, S3).

This task is only about packaging and launch configuration: getting `plugin/skills/*` in front of each tool. Leave
the copy code and the way prompts name skills alone; T2 does those.

## What to change

- **One source of truth.** The skills stay in `plugin/skills/<name>/` in the repo. Don't duplicate them in git.
  Each tool's plugin folder must present them as its own `skills/` folder:
  - `<plugin>/claude/skills/`
  - `<plugin>/copilot/skills/`
  - Codex's fixed plugin folder `runtime/codex-plugin/skills/`, refreshed by `codex::setup::refresh_plugin`.

  Pick the mechanism that works on every platform, Windows included. Symbolic links need admin rights there, so
  don't rely on links in git. The likely best place is where Otaq already prepares plugin folders at runtime:
  - the per-build runtime copy (`crate::runtime`, `DataDir::runtime_dir`);
  - `refresh_plugin` for Codex.

  Fill each tool's `skills/` there from the shipped skills. Dev builds also find the repo's `plugin/` directly
  (`claude::launch::find_plugin_dir`, last in its list), so make sure a dev build ends up with the skills in each
  plugin too. Explain your choice in the module comment.
- **OpenCode:** add `"skills": ["<plugin>/skills"]`, or the form OpenCode 2.x documents, to the
  `OPENCODE_CONFIG_CONTENT` that `opencode/launch.rs` builds. The format used before T-0408 is in
  `git show 49604d8f^:crates/otaq-core/src/agent_tools/opencode/launch.rs`. Update that module's header comment.
- **Manifests:** fix the `description` in `plugin/claude/.claude-plugin/plugin.json`,
  `plugin/copilot/.claude-plugin/plugin.json` and `plugin/codex/.codex-plugin/plugin.json`. They now say "hooks
  only… Otaq copies the skills". Say they carry Otaq's hooks and skills. **Don't change the Codex hook command lines**:
  Codex trusts each hook by a hash of its config.
- **Codex setup:** `refresh_plugin` copies the skills in. Check that `codex plugin add otaq@otaq-local` refreshes
  Codex's cached copy when the skills change. The module comment says it does: keep that behaviour and test it with
  the stand-in `codex`. Update the module comment ("its hooks only" and the skills sentence).
- **Bundle and smoke test:** `crates/otaq-desktop/tauri.bundle.conf.json` already bundles `plugin/`. Extend
  `scripts/release/smoke.sh` so it checks each tool's plugin gets its skills, in whatever form your mechanism
  produces.

## Tests

- Unit or suite tests:
  - Claude Code's and Copilot's plugin folders hold `skills/<name>/SKILL.md` for every shipped skill
    (`agents::shipped`'s list) once the runtime copy is made;
  - Codex's refreshed folder has them;
  - OpenCode's config JSON carries the skills path.
- Existing launch-argument tests for the four tools still pass. Update any test that pins the old OpenCode config.

## Done when

- `scripts/check.sh lint test` passes.
- A shipped skill exists in exactly one place in git.
- Report the mechanism you chose and how it behaves on Windows.
