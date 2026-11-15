# Tech spec · skills-in-their-own-repos

- **Date:** 2026-11-14
- **Decisions:** 057, 062, 063, 064, 065, 069, 071, 072, 073, 074, 075, 077, 079, 080, 081, 082, 083, 089, 090, 092, 093, 094, 098, 100, 102, 103, 104, 105, 106, 107, 108, 109

## How it is used

```rust
// Before a stage starts, or before Otaq types a tuq: skill into a running session (103, 083)
match tuq.ensure(tool, "tuq:security-check").await {
    Tuq::Ready { version } => launch(...),            // stored with the session for History (069, 102)
    Tuq::Missing { reason } => hold(MissingSkill),     // "Your tuq doesn't have `tuq:update-docs`"
}
// Settings → About and the connected-tools list
tuq.versions()  // [(Claude Code, Some("2026.11.12")), (Codex, None), …]
```

```sh
# What Otaq runs to install or update tuq on a tool, and what tuq's README tells people to run (106)
curl -fsSL https://raw.githubusercontent.com/otaq-io/tuq/main/install.sh | sh -s -- --tool codex
irm https://raw.githubusercontent.com/otaq-io/tuq/main/install.ps1 | iex   # Windows; same flags
```

```text
Pick a skill…   validate             Personal · Claude Code · Checks the build
                ── Otaq ──────────────────────────────────────────
                otaq:validate    [Otaq]  Checks the work against the ticket
                tuq:product-spec [Otaq]  Settles what to build, one question at a time
```

## How it is built

**Workflow skills stay where they already are: inside the app.** Today the app ships `plugin/skills/`, and the
runtime copy fills it into each tool's plugin (`runtime.rs`); decision 102 keeps exactly that. What changes: the
skills' source moves to `otaq-io/otaq-skills` (057) and comes back into the Otaq repository as a **git submodule at
`plugin/skills/`**, pinned to a commit; `product-spec`, `tech-spec` and `milestone-plan` leave it (they are tuq's now).
The runtime copy also writes the **prefixed copy** (`otaq-<name>`, `name:` and references rewritten) and points
Copilot's plugin and OpenCode's `skills` path at it (092, 079); Claude Code and Codex keep the plain folder under the
`otaq` plugin. There is no fetching, no signing and no store: a skill fix ships with the next app build.

**tuq is the user's, installed and updated by tuq's own installer.** A new `TuqInstalls` part in otaq-core answers
one question per tool: does this tool's tuq have skill X? It reads the tool's installed tuq through the skill finder
(`skills/found.rs`, which already reads every tool's plugins and skill folders) and its version from the
`metadata.tuq-release` stamp each tuq `SKILL.md` carries. When the skill is missing, it runs tuq's installer for that
tool (`install.sh` / `install.ps1` from `otaq-io/tuq`), which installs or updates tuq the tool's normal way and puts
`tuq`/`tuq-plan` in `~/.tuq/bin` (Windows `%LOCALAPPDATA%\tuq\bin`), on the user's PATH; then it checks again. The
knowledge of how tuq installs on each tool lives once, in tuq's installer, which people without Otaq run too.

**Launches change little.** The tool loads the user's tuq itself, so Otaq passes nothing for it; it keeps passing its
own plugin as today, adds `~/.tuq/bin` to the session's PATH (a PATH change made by the installer doesn't reach the
already-running service) and pre-approves `tuq` and `tuq-plan` like `otaq` (077). A stage types `/otaq:work-ticket`,
then the stage's task skill as a hand-off (§4).

**Around it:** flows are rewritten once to `otaq:`/`tuq:` names (075); the skill picker groups and badges Otaq's skills
(108); About and History show the tuq version per tool and per session (100, 102); Codex's disconnect keeps removing
Otaq's plugin and leaves tuq (104). In the tuq repository: the Rust `tuq`/`tuq-plan` commands built per platform as
GitHub release assets, the installer, the generated prefixed plugin for Copilot and OpenCode (105), the skills'
content changes (081, 093, 094, 098) and `tuq rules` naming AGENTS.md (071).

## What changes

```text
otaq-io/otaq-skills [new] --submodule--> plugin/skills [moved]
runtime.rs [changed] --> claude/ + codex fixed folder (plain) | copilot/ + opencode (prefixed copy [new])

start / hand-offs [changed] --> TuqInstalls [new] --> skills/found.rs [changed] (tuq per tool, version stamp)
                                     |          '--> tuq installer [new, otaq-io/tuq] --> tool's plugin or skills folder
                                     v                                                  + ~/.tuq/bin
                              agents/gate.rs missing skill [changed]
AgentTools::launch [changed]: + ~/.tuq/bin on PATH, tuq pre-approved
flow store [changed] --> rename-to-namespaces migration [new]
flow_editor api + FlowEditor.tsx [changed] <-- skills/found.rs (Otaq group, badge)
session record [changed]: tuq version --> History, About [changed]
```

| Part              | Where                                                                   | New / changed | What it does                                                                      |
| ----------------- | ----------------------------------------------------------------------- | ------------- | --------------------------------------------------------------------------------- |
| workflow skills   | `plugin/skills/` (submodule of `otaq-io/otaq-skills`)                    | changed       | the skills' source moves out; pinned per Otaq commit; tuq's three leave            |
| runtime copy      | `otaq-core/src/runtime.rs`                                               | changed       | also writes the prefixed copy for Copilot and OpenCode                             |
| Copilot, OpenCode | `agent_tools/copilot/launch.rs`, `agent_tools/opencode/launch.rs`        | changed       | point at the prefixed copy; `/otaq:otaq-validate` form (092)                       |
| TuqInstalls       | `otaq-core/src/tuq/mod.rs`                                               | new           | `ensure(tool, skill)`, `versions()`, runs tuq's installer, one install at a time  |
| skill finder      | `otaq-core/src/skills/found.rs`                                          | changed       | finds tuq per tool with its version; groups Otaq's skills (`otaq:`, `tuq:`)        |
| missing skill     | `otaq-core/src/agents/gate.rs`, `board/missing_skill.rs`                  | changed       | asks `TuqInstalls` before holding; 083's message names "Your tuq"                  |
| launches          | `otaq-core/src/agent_tools/*/launch.rs`                                   | changed       | `~/.tuq/bin` on PATH; pre-approve `tuq`, `tuq-plan`                                |
| stage start       | `otaq-core/src/start/`, `session/hand_offs.rs`                            | changed       | types `/otaq:work-ticket`, then the task skill                                     |
| flow migration    | `otaq-core/src/flow/` (store, presets)                                    | changed       | one-time rename to `otaq:`/`tuq:`; on Back and import too; drops Test in browser   |
| picker            | `otaq-desktop/src/api/flow_editor.rs`, `ui/src/screens/flow_editor/`      | changed       | "Otaq" group with icon, badge per row                                              |
| About, History    | `otaq-desktop/src/about.rs`, `api/sections.rs`, session record             | changed       | tuq version per tool and per session                                               |
| tuq repository    | `otaq-io/tuq`: `install.sh`, `install.ps1`, `commands/`, prefixed plugin | new           | installer, Rust commands, Copilot/OpenCode plugin with `tuq-<name>`                |

## Architecture decisions

### 1. Workflow skills come in as a git submodule

The Otaq repository pins `otaq-io/otaq-skills` at `plugin/skills/`; a build ships whatever commit is pinned, and a
change is a commit in the skills repository plus a pointer bump in Otaq (the cost 057 accepted). Considered: a lock
file naming a commit and a build step that fetches it. Lost: it rebuilds what git's submodules already do, and a
contributor would need network access at build time instead of `git clone --recursive`.

### 2. One `otaq` plugin keeps carrying hooks and workflow skills

Since 102 ships the workflow skills with the app, like the hooks, both stay in the one plugin each tool gets today;
the `otaq-hooks` rename (082) isn't done. 082 renamed the hooks plugin only because the workflow skills were to be a
separate plugin named `otaq`; nobody types the name (082), and keeping it avoids re-recording Codex's hook trust.
Considered: splitting as 082 says. Lost: a second plugin per tool and a trust migration on Codex, for no gain now.

### 3. tuq's installer is the one way tuq gets installed

`TuqInstalls` runs tuq's own installer per tool instead of knowing each tool's install commands itself, and people
without Otaq run the same installer from tuq's README (106). Considered: Otaq running `claude plugin install`,
`codex plugin add`, `copilot plugin install` and copying OpenCode skill folders itself. Lost: a second copy of "how tuq
installs on each tool" that drifts from tuq's README.

### 4. A stage types the workflow skill, then the task skill

Otaq starts the session with `/otaq:work-ticket` and, once that turn ends, types the stage's task skill
(`/tuq:product-spec`) as a hand-off, as it already types skills into running sessions. Considered: the workflow skill
calling the task skill. Lost: on Claude Code and Copilot a skill can't call a person-only skill (hands-on checks), and
a user's own task skill may be marked person-only.

### 5. tuq's version comes from a stamp in each SKILL.md

tuq's release script writes `metadata.tuq-release: 2026.11.12` into every `SKILL.md`; the finder reads it on every
tool, so About, History and "is this tuq?" work the same on OpenCode (skill folders, no manifest) as on the plugin
tools. Considered: each tool's plugin manifest version. Lost: OpenCode has none, so it would be a second way.

### 6. tuq's commands are release assets, installed into `~/.tuq/bin`

The Rust `tuq` and `tuq-plan` are built per platform and attached to each tuq GitHub release; the installer
downloads the platform's pair into `~/.tuq/bin` and puts that folder on the user's PATH, the same folder on every
tool. They are signed as the app is (107). Considered: committing the binaries in the repository for a plugin's
`bin/`. Lost: the repository grows with every change, and `bin/` reaches the PATH only on Claude Code.

## Testing

- `TuqInstalls` with a stand-in installer and stand-in tool folders: present, missing then installed, still missing
  after the installer, installer failing (offline), two stages asking at once (one install).
- Finder: tuq found per tool with its stamp; loose and repository copies are the user's own (100); Otaq group.
- Runtime copy: the prefixed copy's names and references, real files only (no links).
- Launch tests per tool: `~/.tuq/bin` on PATH and the pre-approval; Copilot and OpenCode on the prefixed copy.
- Flow migration: a 0.2 flow with Otaq's and custom names, Back, import.
- tuq installer: each tool on macOS, Linux and the Windows VM, fresh and over an older install.
- Hands-on (VM and Mac): which typed form runs a prefixed plugin skill on Copilot (`/otaq-validate` or
  `/otaq:otaq-validate`); OpenCode typing `otaq-validate` interactively; a user's own `otaq` plugin installed in Codex
  beside Otaq's (turned off per run with `-c plugins.<id>.enabled=false`, unquoted).

## Risks

- **Otaq runs a downloaded installer** with the user's rights when a stage needs tuq. It comes from `otaq-io/tuq`
  over HTTPS with no signature of its own (the signing decisions were superseded by 102); whoever controls that
  repository controls what runs.
- **An update in the middle of the user's own work.** Updating tuq for a stage changes the tuq the user's open
  terminals use; 103 accepts replacing their edits inside tuq's files.
- **The service's PATH.** The installer's PATH change reaches new terminals, not the running service; Otaq adds
  `~/.tuq/bin` itself.
- **Managed machines** may block unsigned `tuq.exe` (107 accepts it for this release).
- **Upgrade start:** flows migrated, `product-spec`/`tech-spec`/`milestone-plan` no longer in Otaq's plugin, so the
  first such stage installs tuq; each step must be safe to run twice.

## Open product points

- **Person-only marks on tuq (089).** 089 put the marks on Otaq's copies of `product-spec`, `tech-spec` and
  `milestone-plan`; under 102 Otaq has no copies, so the user's tuq runs unmarked in Otaq's sessions and only the rule
  in words keeps Product spec from starting by itself. Does tuq itself carry the marks (changing how it behaves in a
  plain terminal), or is the rule enough? The design works either way.
