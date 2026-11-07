# Stand-in agent tool

`agent-tool.sh` stands in for `claude`, `codex`, `copilot` and `opencode` in tests (T-0487, decision 0118, T2). It
behaves like the real tools in the ways hand-offs need: **Otaq's own skills come from Otaq's plugin**, so every one of
them is there from the start, and **the user's project skills are read only at start**.

- The tool is the script's own name (`claude`, `codex`, `copilot`, `opencode`: install it under those names, as
  symlinks or copies), or `OTAQ_STAND_IN_TOOL`.
- At start it lists the project skills in the folder it starts in (`.claude/skills/*/SKILL.md` as `claude`,
  `.agents/skills/*/SKILL.md` as the other three) and prints `STAND-IN <tool> loaded: team-validate … `, then
  `ARGS <its arguments>`.
- Its first prompt is its last argument when that names a skill; then each line typed into its terminal (a bracketed
  paste too). For each:
  - Otaq's skill in the tool's form (`/otaq:x` for `claude` and `copilot`, `$otaq:x` for `codex`, a line starting
    "Load the skill `x` …" for `opencode`): `RUNS otaq:x` (`RUNS x` for `opencode`) when `x` is one of Otaq's shipped
    skills, else `Unknown command: /otaq:x`;
  - the user's skill (`/x`, `$x`, or `x` in `opencode`'s "Load the skill `x` …"): `RUNS x` when it was loaded at
    start, else `Unknown command: /x` (`opencode`: `No skill named x.`);
  - any other text: nothing.
- With `OTAQ_STAND_IN_HOOKS=1` in its environment, as `claude` only, a prompt it runs sends Claude Code's
  `UserPromptSubmit` and then `Stop` hooks through `otaq hook --tool claude …` (the `otaq` on its PATH: the launch
  puts Otaq's own first). An `Unknown command` sends nothing, as the real tool's turn never starts. Off by default.
  Other tools send no hooks.
- It runs until its terminal closes.

## Driving it

From `otaq-core`'s tests: `crate::common::stand_in::install(bin)` puts the four names into `bin` and
`stand_in::launch(bin, plugin)` gives the service's `LaunchContext` (its `claude` is `bin/claude`). Wait for
`RUNS otaq:restack` / `Unknown command` in the session's output (`TestService::wait_for_output`).

By hand, or from an end-to-end check of the installed app: symlink the script as `claude` (and the others) into a
folder put first on the service's `PATH`, with a throwaway `HOME` and `OTAQ_HOME`, and set `OTAQ_STAND_IN_HOOKS=1`
in the service's environment for the hooks. Never install it over a real tool.
