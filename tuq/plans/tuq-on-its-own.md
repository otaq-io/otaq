# Plan · tuq-on-its-own

- **Ticket:** none
- **Date:** 2026-11-14
- **Decisions:** 064, 065, 071, 073, 074, 077, 080, 081, 093, 094, 100, 105, 106, 107
- **Tech spec:** tuq/tech-specs/skills-in-their-own-repos.md
- **Orchestrator prompt:** tuq/prompts/tuq-on-its-own-orchestrator.md

<!-- tuq:plan:start -->

tuq is made to stand on its own, in its own repository (`~/Desktop/Workspace/tuq`, to become `otaq-io/tuq`), in
four pieces, one behaviour each. The skills losing everything Otaq-specific and the Rust commands come first, side by
side (decisions 081, 093, 094, 077); the release, which stamps the skills and ships the commands, needs both; the
installer needs a release to install. Otaq's own plan (tuq/plans/skills-in-their-own-repos.md) builds on a published
tuq release. The agent in charge follows tuq/prompts/tuq-on-its-own-orchestrator.md.

## Pieces

1. tuq's skills work without Otaq — waits for nothing
   In `plugins/tuq/`: `product-spec`, `tech-spec`, `milestone-plan` and `step-back` lose every Otaq part ("Which
   mode", `otaq prime`, `otaq ask human`, `otaq stage next`, `$OTAQ_*`, "In an Otaq stage") and keep their plain-chat
   behaviour as the only one (decisions 081, 094). No skill names `tuq-plan pull` or `body`, nor the ticket's Plan
   (decision 093): the plan file stands alone. Skills name each other in the namespaced form ("Call the Skill tool
   with `tuq:step-back`", decision 074). `rules.md` adds the pointer to AGENTS.md as well as README and CLAUDE.md,
   only where each exists (decision 071), and drops "the ticket's Plan word for word". `browser-validate` leaves
   `.claude-plugin/marketplace.json`, whose name stays `otaq-claude-skills` and points `tuq` at `./plugins/tuq`
   (decisions 065, 080). The skills' text only; the commands are piece 2.
   Check: `grep -rn "otaq \|OTAQ_\|tuq-plan pull\|tuq-plan body" plugins/tuq/skills plugins/tuq/rules.md` finds
   nothing; `/tuq:product-spec` in a plain Claude Code chat runs and writes a decision.
2. tuq and tuq-plan run as Rust programs on every system — waits for nothing
   A `commands/` Rust crate in the tuq repository builds `tuq` (rules, template, init, next, index, clashes) and
   `tuq-plan` (new, check, order) with the same behaviour and messages as today's bash scripts in
   `plugins/tuq/bin/`, minus `pull` and `body` (decision 093). `rules.md` and `templates/` are built into the
   programs, so a program and its skills always match. No bash, python3 or other runtime needed (decisions 064,
   077). The bash scripts go once the programs pass the same tests. Tests run each command on a scratch repository,
   compared with the bash scripts' output before they go.
   Check: the crate's tests pass on macOS and in the Windows VM (PowerShell, no Git Bash on PATH); `tuq next`,
   `tuq index` and `tuq-plan check` give the bash scripts' answers on this repository's `tuq/`.
3. The maintainer publishes a tuq release — waits for 1, 2
   `scripts/release` in the tuq repository: takes the version from today's date (`2026.11.14`, `.2` for a second
   that day), writes `metadata.tuq-release: <version>` into every `SKILL.md` (tech spec §5), generates the prefixed
   plugin for Copilot and OpenCode (`tuq-<name>` folders and `name:`, references to other tuq skills rewritten,
   decision 105) from `plugins/tuq/`, cross-builds `tuq` and `tuq-plan` for macOS (universal), Windows and Linux
   (x64, arm64), signs them as Otaq's app is signed (ad-hoc on macOS, none on Windows, decision 107), commits, tags
   and creates the GitHub release with the binaries as assets (tech spec §6). A check fails when the committed
   prefixed plugin doesn't match what generation gives. `--dry-run` does everything but push and release.
   Check: `scripts/release --dry-run` leaves stamped skills, an up-to-date prefixed plugin and the six binaries; the
   stale-plugin check fails after a skill is edited and not regenerated.
4. tuq installs itself on any tool with one command — waits for 3
   `install.sh` (macOS, Linux) and `install.ps1` (Windows) at the repository root, `--tool claude|codex|copilot|opencode`:
   install or update tuq the tool's normal way (Claude Code: the `otaq-claude-skills` marketplace and `tuq` plugin;
   Codex and Copilot: their plugin commands; OpenCode: its skills folder), the prefixed plugin on Copilot and
   OpenCode (decision 105); download the platform's `tuq` and `tuq-plan` from the latest release into `~/.tuq/bin`
   (`%LOCALAPPDATA%\tuq\bin`) and put that folder on the user's PATH once (decision 106). Running it again updates; it
   prints the installed version and exits non-zero with a reason when it can't install. The README says what tuq is,
   how to install it on each tool with these commands, and how Otaq uses it; the "Otaq copies SKILL.md" line and the
   "tug" spelling go (decision 073).
   Check: on the Mac and in the Windows VM, for each of the four tools, a fresh install and an install over an older
   tuq both end with `tuq:product-spec` (Copilot and OpenCode: `tuq-product-spec`) listed by the tool and `tuq next`
   working in a new terminal.

<!-- tuq:plan:end -->
