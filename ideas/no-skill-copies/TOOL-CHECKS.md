# Tool checks for the Tech spec: Otaq's skills as plugins on four tools

Hands-on checks run on 2026-11-14 to settle what the tuq decisions 057–091 assume about the agent tools. Nothing in
Otaq or tuq was changed. Versions: Claude Code 2.1.325 (macOS); in the Windows VM (Windows 11 arm64): codex-cli
0.166.0, GitHub Copilot CLI 1.0.109, OpenCode 2.2.5. Test plugins used answers that can't be guessed, so a right
answer proves a skill was loaded.

## For the Tech spec

What the design must take from these checks, by decision.

- **One plugin file for three tools (074): confirmed.** Codex loads a plugin that has only `.claude-plugin/plugin.json`
  and `skills/`, installed through a local marketplace as Otaq does today, and names its skills `$<plugin>:<skill>`.
  Copilot reads the same file. No separate `.codex-plugin/` is needed.
- **No symlinks or junctions anywhere (074): confirmed and required.** Codex drops symlinked files, symlinked folders
  and junctions when it installs a plugin, without a message; the skill then silently doesn't exist. Every per-session
  or per-release copy must be real files.
- **A copy per session on Codex (061, 091): possible two ways.** (a) A Codex home per session: works, two sessions ran
  in parallel on different copies. (b) One Codex home: install each release's copy from its own local marketplace and
  enable exactly one per run with `-c plugins."<id>".enabled=true|false` (or a profile, `-p`); works even with no
  `[plugins]` table in `config.toml`. With two copies enabled at once, one wins silently, so the launch must disable
  every other copy. There is no per-run setting that points Codex at a skills folder (`skills.config` only toggles
  skills Codex already found). **Not checked:** each copy has its own plugin id, so Otaq's hook trust (today keyed
  `otaq@otaq-local:…`) must be recorded per id, and hooks live in `otaq-hooks` (082), which stays one copy.
- **Several plugins per session (082): confirmed** on Claude Code and Copilot (`--plugin-dir` repeated). On Codex,
  several installed plugins load together.
- **Person-only marks and chains (089).**
  - Claude Code and Copilot: a `disable-model-invocation: true` skill is never self-started and runs when typed, but
    **a call to it from another skill is hard-blocked**, even when a person started the caller. So skills that others
    call (`commit`, `step-back`, a user's `grilling`) must stay unmarked, as 089 says; and nothing may later call the
    newly marked `product-spec`, `tech-spec` or `milestone-plan`, or it breaks on these two tools.
  - Codex: `agents/openai.yaml` with `policy.allow_implicit_invocation: false` hides the skill from the model and it is
    never self-started; typed `$plugin:skill` runs it. Codex has no skill tool, so "another skill calls it" is
    undefined there (one caller gave up, another found the file and read it).
  - OpenCode: `disable-model-invocation: true` (or `metadata: opencode/autoinvoke: false`) only removes the skill from
    the model's list; the skill tool still loads it if the model names it. On OpenCode the rule in words (089) is the
    real protection.
- **Typed skill names (074).** Claude Code `/plugin:skill` and Codex `$plugin:skill` confirmed. Copilot's skill tool
  takes both `plugin:skill` and bare `skill`, and the model sees bare names; whether a typed `/plugin:skill` runs it
  could not be shown in headless runs (it reached the model as text). OpenCode `tuq-product-spec` loads from the
  `skills` path in `OPENCODE_CONFIG_CONTENT` and is picked by name; typed `/name` under `opencode run` was
  inconclusive. **Both need an interactive check**, as Otaq types skills into interactive sessions.
- **Disconnect (090): needs building.** Today `remove_setup` (`codex/setup.rs`) removes only the fixed `otaq@otaq-local`
  plugin: `codex plugin remove`, `marketplace remove`, its two `config.toml` tables, hook trust with that prefix, and
  `rules/otaq.rules`; folder trust stays. The `otaq`, `tuq` and `otaq-hooks` plugins and every per-session copy must be
  covered. `codex plugin remove` also leaves an empty `plugins/cache/<marketplace>/` folder, a small trace against
  "no trace of Otaq"; Otaq's real disconnect wasn't run to confirm it leaves one too.

## Still to check (not covered here)

- Hook trust per Codex copy id (above).
- Copilot and OpenCode typing a namespaced skill in an interactive session.
- Codex hooks with several `otaq*` plugins installed in one home.

## The full report

## Verdicts

| # | Check | Verdict |
|---|-------|---------|
| 1 | Codex loads a plugin that has only `.claude-plugin/plugin.json` + `skills/` | **Yes.** It installs, loads the skills, and names them `<manifest name>:<skill>` (`$pw:secret-word`) |
| 2a | Two Codex sessions at once, each with its own CODEX_HOME and a different copy | **Yes** (ALPHA and BRAVO in parallel) |
| 2b | Same CODEX_HOME, a different copy per session, picked by a launch flag | **Yes.** Install each copy from its own marketplace; then `-c plugins."<id>".enabled=true/false` (or `-p <profile>`) picks one per run. This works even when `config.toml` has no `[plugins]` entry at all |
| 2c | A config key that points Codex at a skills folder per run (`skills.config` path) | **No.** It didn't add the skill |
| 2d | Codex keeps symlinks when it installs a plugin | **No.** It drops symlinked files, symlinked folders and junctions without a word |
| 3a | Codex `allow_implicit_invocation: false` stops the model from starting the skill itself | **Yes.** The skill isn't listed to the model |
| 3b | Typed `$pw:magic-number` still runs | **Yes** |
| 3c | Another skill can still call it | **Unclear (inconsistent).** Codex has no skill tool. Typed `$pw:run-caller`: the model said it couldn't. Model-started caller: the model found the hidden SKILL.md by `rg` and read it |
| 4a | Claude Code: several `--plugin-dir` at once | **Yes** |
| 4b | `disable-model-invocation` blocks the model starting the skill itself | **Yes.** Not offered to the model, and it said no skill covers it |
| 4c | Typed `/pw:magic-number` runs | **Yes** |
| 4d | Another skill can call a person-only skill through the Skill tool | **No.** Hard-blocked with a tool error, whether the caller was typed or model-started |
| 4e | A model-invocable skill calls another model-invocable one | **Yes** |
| 5a | Copilot: several `--plugin-dir` at once | **Yes** |
| 5b | Copilot honours `disable-model-invocation` for plugin skills | **Yes.** Not self-started, and a call from another skill is blocked ("Skill model invocation disabled") |
| 5c | Copilot name form `/plugin:skill` | **Partly.** The skill tool accepts both `pw:magic-number` and bare `run-caller`, and the model saw bare names. A typed slash command couldn't be tested headless (`-p`/`-i` without a TTY sends it to the model as text) |
| 6a | OpenCode: `tuq-product-spec` loads from a `skills` path in `OPENCODE_CONFIG_CONTENT` and is picked by name | **Yes** |
| 6b | OpenCode: a way to stop the model starting a skill itself | **Yes, partly.** Front matter `disable-model-invocation: true` (or `metadata: opencode/autoinvoke: false`) leaves the skill out of the model's list. **But** the skill tool still loads it when the model names it, so it is not a block |
| 7 | Codex disconnect covers a 2nd and 3rd plugin | **No, not today** (code reading). Removal is hard-wired to the single `otaq@otaq-local` id, its marketplace, its hook-trust prefix and `rules/otaq.rules`. Every new plugin or marketplace needs its own entry |

## Fixtures (shared by all checks)

Throwaway plugins, each with only `.claude-plugin/plugin.json` (no `.codex-plugin/`):

- `pw-a/` (`name: "pw"`) and `pw-b/` (`name: "pw"`). They are identical except `secret-word`, which answers `SECRET=ALPHA` in pw-a and `SECRET=BRAVO` in pw-b. Skills:
  - `secret-word`: plain skill.
  - `magic-number`: person-only. It has `disable-model-invocation: true` and `agents/openai.yaml` with `policy.allow_implicit_invocation: false`, and answers `MAGIC-7731`.
  - `run-caller`: calls `pw:magic-number`, otherwise replies `CALLER-BLOCKED <reason>`.
  - `helper-code`: answers `HELPER-4242`.
  - `chain-ok`: calls `pw:helper-code`.
- `pg/` (`name: "pg"`): `fav-colour` answers `TEAL-55`.
- `oc/skills/`: `tuq-product-spec` (`SPEC-9090`) and `tuq-magic` (person-only via `disable-model-invocation: true`). These were used for OpenCode and for Copilot's `COPILOT_SKILLS_DIRS`.

The descriptions invite automatic use ("Use whenever the user asks for the magic number"). The answers can't be guessed, so a correct answer proves the skill was loaded.

---

## 1. Codex reads a Claude manifest

**Commands.** I reproduced Otaq's install from `codex/setup.rs`: a local marketplace in the plugin folder, `.agents/plugins/marketplace.json` = `{"name":"pwa-local","plugins":[{"name":"pw","source":{"source":"local","path":"./"},"policy":{...}}]}`. CODEX_HOME was `%TEMP%\toolchk\ch1`, with `auth.json` copied in.

```
codex plugin marketplace add %TEMP%\toolchk\fx\pw-a
codex plugin add pw@pwa-local
codex exec --json --skip-git-repo-check "List every skill ... whose name contains pw or pg ... Then answer: what is the secret word?"
```

**Output.**
- `config.toml` got `[marketplaces.pwa-local]` and `[plugins."pw@pwa-local"] enabled = true`.
- The installed copy is `plugins\cache\pwa-local\pw\0.1.0\` (`.claude-plugin\plugin.json`, `skills\...`, `.agents\...`). The version folder comes from the Claude manifest's `version`.
- `codex plugin list`: `pw@pwa-local  installed, enabled  0.1.0`.
- The model listed: `pg:fav-colour`, `pw:chain-ok`, `pw:helper-code`, `pw:run-caller`, `pw:secret-word`. `magic-number` is absent: see check 3.
- With `-s danger-full-access` (see note): `SECRET=ALPHA`, after reading `...\cache\pwa-local\pw\0.1.0\skills\secret-word\SKILL.md`.

**Note.** In a fresh CODEX_HOME the default `codex exec` sandbox on Windows refused the model's `Get-Content` of the skill file ("blocked by policy"). All later runs used `-s danger-full-access` inside throwaway folders. Otaq's real interactive launch uses the user's own settings, so this doesn't apply there.

**Meaning.** Decision 074's line "one `.claude-plugin/plugin.json` serves Claude Code, Codex and Copilot" holds for Codex 0.166. The `mattpocock/skills` claim is confirmed. The namespace is the manifest `name`, not the folder or marketplace name, so `otaq:` and `tuq:` come out as `$otaq:validate` and `$tuq:product-spec`, as 074 says. Otaq still needs a marketplace file next to the plugin to install it, as it does today.

## 2. Codex, a copy per session

| Run | Setup | Answer |
|-----|-------|--------|
| p-ch1 ∥ p-ch2 | CODEX_HOME `ch1` (pw-a) and `ch2` (pw-b), started at the same time | ALPHA / BRAVO |
| t-both | `ch3`: `pw@pwa-local` **and** `pw@pwb-local` both installed and enabled | ALPHA (with two copies enabled, one silently wins) |
| t-aonly ∥ t-bonly | `ch3`, at the same time: `-c plugins.pw@pwb-local.enabled=false` vs `-c plugins.pw@pwa-local.enabled=false` | ALPHA / BRAVO |
| t-off | `ch3`, both disabled by `-c` | no skill ("I don't see any files…") |
| prof-b | `ch3`, `-p bonly` (`bonly.config.toml` = `[plugins."pw@pwa-local"] enabled=false`) | BRAVO |
| e-none | `ch6` = copy of ch3 with **all `[plugins]` tables removed** from config.toml; caches and marketplaces kept | no skill |
| e-bon | `ch6` with `-c plugins.pw@pwb-local.enabled=true` | **BRAVO**, read from `ch6\plugins\cache\pwb-local\...` |
| s-config | empty `ch4`, `-c "skills.config=[{path='.../pw-b/skills/secret-word/SKILL.md',enabled=true}]"` | no skill |

Command shape: `set CODEX_HOME=<home>&& codex exec --json --skip-git-repo-check -s danger-full-access [-c …] "What is the secret word?"`.

`codex exec --help` (0.166) also shows `-p/--profile` ("Layer $CODEX_HOME/<name>.config.toml on top of the base user config") and `--ignore-user-config`. The strings in the binary show `SkillsConfig { bundled, include_instructions, max_context_tokens, config }`. The `skills.config` entries take "a path or name selector", so they only turn already-found skills on or off. No per-run skills-folder key was found.

The `ch6` cache was still there after several runs with no `[plugins]` entry. Codex didn't garbage-collect it in this test, though only a few launches were tried.

**Symlinks.** I made `pw-s` (a copy of pw-a). Its `secret-word/SKILL.md` was a file symlink to pw-b's, plus a `linked-helper` folder symlink and a `junction-helper` junction. The symlinks were made with admin rights, because they need them on this VM. After `codex plugin add pw@pws-local`, the installed copy had an **empty** `skills\secret-word\` folder and no `linked-helper` or `junction-helper`. The model then had no `secret-word` skill. Codex drops all three link kinds and prints nothing.

**Meaning for 078.**
- 078 assumed Codex might not give each session its own copy. It can, in one home:
  - Install each bundle under its own marketplace name (`otaq-<bundle>`, the plugin still named `otaq`, so the skills stay `$otaq:…`).
  - Leave or remove the `[plugins]` entry.
  - Launch each session with `-c plugins."otaq@otaq-<bundle>".enabled=true`.
  - Resumes would pass the same flag.
- Caveats, not tested:
  - Hook trust is keyed by plugin id (`otaq@otaq-local:plugin.json#hooks…`), so every bundle id needs its own trust record.
  - Each `codex plugin add` rewrites `config.toml`.
  - Whether a plain `codex` in the user's terminal would see an installed-but-unconfigured cache is the e-none case: it doesn't.
  - Two copies enabled at once is a silent tie (ALPHA won).
- Per-session CODEX_HOME also works, but it moves auth, sessions and history away from the user's home.
- The "switch only when idle" rule in 078 may be avoidable.

**Meaning for 074.** "No symlinks (Codex drops them)" is confirmed, junctions included.

## 3. Codex, person-only (`agents/openai.yaml` `allow_implicit_invocation: false`)

All runs in `ch1` (pw-a installed).

| Run | Prompt | Result |
|-----|--------|--------|
| list (check 1) | list skills | `magic-number` not listed; the other four are |
| c-magic | "What is the magic number?" | `42.`: not started |
| c-typed | `$pw:magic-number` | `MAGIC-7731` |
| c-caller | `$pw:run-caller` (typed) | "CALLER-BLOCKED: The skill tool for starting `pw:magic-number` is unavailable." |
| c-implcall | "Please run the caller." (model-started) | Read `run-caller/SKILL.md`, ran `rg --files … \| rg 'magic-number\|SKILL.md'`, read `magic-number/SKILL.md`, answered `MAGIC-7731` |
| c-chain | `$pw:chain-ok` | Read `helper-code/SKILL.md`, then `CHAIN: HELPER-4242` |

**Meaning.**
- For 074 and 089: the openai.yaml mark does what 089 needs. A person-only skill never self-starts on Codex, and a typed `$otaq:…` runs it.
- Codex has no skill tool. A "call" from one skill to another means the model reads the other SKILL.md from disk.
  - Chains to unmarked skills work (`chain-ok`).
  - Chains to a marked skill aren't blocked by Codex. Whether one happens depends on the model looking for the file.
- So the `mattpocock` claim "a person-only skill can't be called by another skill" isn't a guarantee on Codex either way. 089 already keeps the skills other skills call (`commit`, `step-back`) unmarked, which is the safe pattern.

## 4. Claude Code, person-only and chains

Run on this Mac:

```
claude -p "<prompt>" --plugin-dir <fx>/pw-a --plugin-dir <fx>/pg --no-session-persistence --output-format stream-json --verbose --allowedTools Skill
```

**Init event.** `plugins` contained `pw@inline` and `pg@inline` (the user's installed `tuq` too). `skills` and `slash_commands` both listed `pw:chain-ok, pw:helper-code, pw:magic-number, pw:run-caller, pw:secret-word, pg:fav-colour`.

| Prompt | Result |
|--------|--------|
| "What is the magic number? And what is the favourite colour? Use your skills if any apply." | `Skill pg:fav-colour` → TEAL-55; "Magic number: I couldn't find it. None of my skills covers a magic number" |
| `/pw:magic-number` | `MAGIC-7731` |
| `/pw:run-caller` | `Skill pw:magic-number` → `<tool_use_error>Skill pw:magic-number cannot be used with Skill tool due to disable-model-invocation. Ask the user to run /pw:magic-number themselves …</tool_use_error>` |
| "Please run the caller." | `Skill pw:run-caller` (model-started), then the same error for `pw:magic-number` |
| `/pw:chain-ok` | `Skill pw:helper-code` → `CHAIN: HELPER-4242` |

**Meaning for 089.**
- Several `--plugin-dir` flags work, so `otaq`, `tuq` and `otaq-hooks` can each be passed per session.
- A person-only skill can be started only by a person, never by another skill, even a typed one.
- So any skill that another skill calls must stay unmarked. 089 already does that (`commit`, `step-back`). The test also covers its chain examples: `grill-me` → `grilling` and `product-spec` → `step-back` work only while the callee is unmarked.
- Watch for any future chain that targets an entry point (`product-spec`, `tech-spec`, `milestone-plan`), now that 089 marks those: it will be hard-blocked on Claude Code and Copilot.

## 5. Copilot CLI

COPILOT_HOME was `%TEMP%\toolchk\cp1`, holding a `config.json` with only `lastLoggedInUser` and `loggedInUsers` copied. The token stays in Windows' credential store, and no credential was copied or printed.

```
copilot -p "<prompt>" --plugin-dir <fx>\pw-a --plugin-dir <fx>\pg --allow-all-tools --no-auto-update --log-dir <tmp>
```

| Run | Result |
|-----|--------|
| list + favourite colour | `skill(fav-colour)` → `TEAL-55`. The model said no skill names contain "pw" or "pg": it sees **bare** names |
| "What is the magic number?" | Searched files, answered `42`: not started |
| `/pw:magic-number` with `-p` | The model got the text and looked up docs: "This build's documentation doesn't cover `/pw:magic-number`". `-p` doesn't run slash commands. `-i` without a TTY did the same |
| `/pw:run-caller` and "Please run the caller." | `skill(run-caller)`, then `skill(pw:magic-number)` → "Skill model invocation disabled: magic-number" → `CALLER-BLOCKED` |
| `/pw:chain-ok` | `skill(chain-ok)` → `skill(helper-code)` → `CHAIN: HELPER-4242` |
| `COPILOT_SKILLS_DIRS=<fx>\oc\skills`, "What is the magic number?" | `42.`: `tuq-magic` (marked) not started; `copilot skill list --json` shows it as `source: custom, enabled: true` |

**Meaning.**
- Several `--plugin-dir` flags work.
- Copilot honours `disable-model-invocation` for plugin skills exactly like Claude Code, including hard-blocking calls from other skills.
- The skill tool resolves both `pw:magic-number` and bare names. A user's own skill named like an Otaq one could therefore clash, unless skills always call the namespaced form, as 074 prescribes.
- I could not verify a typed `/otaq:skill` from a headless run. Otaq already launches `copilot … -i "/otaq:work-ticket"` (work-ticket is person-only) in a real terminal, so that is the place to confirm it.
- "Skills only inside the plugin folder": the mark is honoured for plugin skills, and a marked skill in a skills directory wasn't self-started either.

## 6. OpenCode

The XDG data, config, state and cache folders pointed into `%TEMP%\toolchk\oc`, with the user's `opencode.db*` copied in for sign-in. I set `OPENCODE_CONFIG_CONTENT={"skills":["<fx>\\oc\\skills"]}`, the form in `opencode/launch.rs`. `opencode debug config` showed it as a `document` source.

```
opencode run --standalone -m google/gemini-3.1-flash-lite --format json "<prompt>"
```

Without `-m` the first run hung with no output, apparently waiting for a model choice, and I stopped it.

| Run | Result |
|-----|--------|
| list + product spec code | The model's skill list contained `tuq-product-spec` and **not** `tuq-magic`. Then `skill {"id":"tuq-product-spec"}` → `SPEC-9090` |
| "What is the magic number?" | Searched files and the web, answered 42: not started |
| "Load the skill tuq-magic with your skill tool and follow it." | `skill {"id":"tuq-magic"}` **loaded** → `MAGIC-7731` |
| `/tuq-magic` as the `run` message (twice) | The model loaded `tuq-product-spec` instead. `opencode run` doesn't treat it as a command; inconclusive for the TUI |
| `skillOverrides: {"tuq-product-spec":"user-invocable-only"}` in config content | No effect: still picked |

The OpenCode 2.2.5 binary contains a front-matter parser that sets `autoinvoke` from `metadata["opencode/autoinvoke"]`, falling back to `disable-model-invocation: true` → `autoinvoke: false`. The skill-guidance list then drops every skill with `autoinvoke === false`. I found no check on the skill tool. The `skillOverrides` string in the binary belongs to an embedded Claude Code settings schema, not to OpenCode's config.

**Meaning.**
- For 072, a prefixed folder and `name:` (`tuq-product-spec`) work and are picked by name.
- For 089, the existing `disable-model-invocation: true` mark already keeps OpenCode from offering the skill to the model.
- It does not stop a model that names the skill. That is weaker than Claude Code and Copilot, though close to what 089 wants, since a chain still works. Otaq's own rule text remains the guard.

## 7. Codex disconnect (code reading)

`crates/otaq-core/src/agent_tools/codex/setup.rs` `remove_setup()`, called from `connected.rs:659` through `Codex::remove_setup`, removes:

1. **The plugin** (`remove_plugin`). If `codex` is on PATH it runs `codex plugin remove otaq@otaq-local` (when `[plugins."otaq@otaq-local"]` is there), then `codex plugin marketplace remove otaq-local` (when `[marketplaces.otaq-local]` is there). Either way it then deletes those two tables from `config.toml` (`config_toml::remove_plugin`). The id and marketplace are fixed: `plugin_id()` = `otaq@otaq-local`, `marketplace()` = `otaq-local`.
2. **Hook trust**: every `hooks.state` key starting with `otaq@otaq-local:` (`config_toml::remove_hooks(doc, &hook_prefix())`).
3. **`rules/otaq.rules`**.
4. Folder trust (`[projects."…"]`) stays (`removed_on_disconnect: false`).

Claude Code, Copilot and OpenCode return an empty report (copilot `trust::remove_setup`, opencode/claude `remove_setup`). That matches 090's "only Codex needs it".

**Second and third plugins.** Not covered. Every removal step is keyed to the one id `otaq@otaq-local`, its marketplace and its hook prefix. A `tuq@…` or `otaq-hooks@…` plugin, or one marketplace per bundle as check 2b suggests, would need:
- the ids generalised to a list (or a prefix such as every marketplace Otaq added);
- hook-trust prefixes for each id;
- a matching `setup_entries`.

The mechanism (run `codex plugin remove`, then strip the tables) works the same for any id.

Extra fact from the VM: `codex plugin remove pw@pwa-local` deletes `plugins\cache\pwa-local\pw\` but leaves an empty `plugins\cache\pwa-local\` folder. `marketplace remove` removes the table only. So after a disconnect today an empty `plugins\cache\otaq-local\` folder probably remains: a small trace against 090's "no trace". Not checked against Otaq's real flow.

---

## Side effects of the checks

All throwaway folders were deleted in the VM and on the Mac. A few read-only OpenCode commands ran against the VM's real
OpenCode home: they added log lines and started OpenCode's background service, which updated its database and
`state/service.json`; no OpenCode settings or skills changed. The Claude Code runs may have updated counters in
`~/.claude.json`.
