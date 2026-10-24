# Stand-in agent tool

`agent-tool.sh` stands in for `claude`, `codex`, `copilot` and `opencode` in tests (T-0487, decision 0118). It
behaves like the real tools in the one way the hand-off race needs: **it reads its skill folders only at start**.

- The tool is the script's own name (`claude`, `codex`, `copilot`, `opencode`: install it under those names, as
  symlinks or copies), or `OTAQ_STAND_IN_TOOL`.
- At start it lists the skill copies in the folder it starts in (`.claude/skills/*/SKILL.md` as `claude`,
  `.agents/skills/*/SKILL.md` as the other three) and prints `STAND-IN <tool> loaded: otaq-commit otaq-restack … `,
  then `ARGS <its arguments>`.
- Its first prompt is its last argument when that names one of Otaq's skills (`/otaq-…`, `$otaq-…`, `otaq-…`) or
  starts with "Follow the skill in"; then each line typed into its terminal (a bracketed paste too). For each:
  - a skill in the tool's form (`/otaq-x` for `claude` and `copilot`, `$otaq-x` for `codex`, `otaq-x` at the start
    of the line for `opencode`): `RUNS otaq-x` when it was loaded at start, else `Unknown command: /otaq-x`;
  - "Follow the skill in `<file>`…": `FOLLOWS <file>` when the file is there now (any file can be read at any time),
    else `No such file: <file>`;
  - any other text: nothing.
- With `OTAQ_STAND_IN_HOOKS=1` in its environment, as `claude` only, a prompt it runs sends Claude Code's
  `UserPromptSubmit` and then `Stop` hooks through `otaq hook --tool claude …` (the `otaq` on its PATH: the launch
  puts Otaq's own first). An `Unknown command` sends nothing, as the real tool's turn never starts. Off by default.
  Other tools send no hooks.
- It runs until its terminal closes.

## Driving it

From `otaq-core`'s tests: `crate::common::stand_in::install(bin)` puts the four names into `bin` and
`stand_in::launch(bin, plugin)` gives the service's `LaunchContext` (its `claude` is `bin/claude`). Wait for
`RUNS otaq-restack` / `Unknown command` in the session's output (`TestService::wait_for_output`).

By hand, or from an end-to-end check of the installed app: symlink the script as `claude` (and the others) into a
folder put first on the service's `PATH`, with a throwaway `HOME` and `OTAQ_HOME`, and set `OTAQ_STAND_IN_HOOKS=1`
in the service's environment for the hooks. Never install it over a real tool.
