# Gap review: Otaq's skills are tuq, released without the app (S9–S26)

Written on 2026-11-14, read-only, on `main` at `9abb6c1d`. The working tree also holds the uncommitted S9–S26 text in
`ideas/no-skill-copies/DECISIONS.md` and their copies as `tuq/decisions/057`–`073`. Apart from this file, nothing
was changed in either repository.

Reviewed: S9–S26 at the end of `ideas/no-skill-copies/DECISIONS.md`, against S1–S8, F1–F19, `tuq/decisions/`
(index in `tuq/README.md`), `ideas/ux-rethink/DECISIONS.md`, `ideas/noyan-intake/DECISIONS.md`, the code, docs and
website, and the sibling clone `otaq-claude-skills` (called **the tuq repo** below). Paths are relative to the Otaq
repository unless they start with `otaq-claude-skills/`. A **bundle** is a signed set of skills the maintainer
publishes (S11). The **built-in skills** are the copy each app build ships (S9). "Sure" says how certain I am that
the gap is real, after checking the code.

## Summary

| Severity | Count | Gaps |
|---|---|---|
| Blocks release | 5 | G1–G5 |
| Confuses users | 5 | G6–G10 |
| Undecided | 10 | G11–G20 |
| Cosmetic | 4 | G21–G24 |
| **Total** | **24** | |

The top five:

1. **G1.** Every existing flow, preset and ready stage names Otaq's skills by their bare names (`skill: validate`).
   Under S15 a bare name is the user's skill, so after the upgrade every ticket would be held for a missing skill.
   Stored flow versions are never rewritten.
2. **G2.** The decisions never compare the built-in skills with a bundle, and a bundle sets only a lowest Otaq
   version. A new app can be moved back to an older bundle, and an old bundle can be run on a newer app that it
   doesn't fit.
3. **G3.** The `tuq` and `tuq-plan` commands are bash scripts, and `tuq-plan pull` also needs python3. Codex runs
   commands in PowerShell on Windows, and only `otaq` is pre-approved on any tool. The decisions don't say where the
   commands come from (a bundle or the app) or how they run without a permission prompt.
4. **G11.** One plugin named `otaq` carries both the hooks and the skills today. The decisions don't say whether the
   hooks stay in an `otaq` plugin next to a new `tuq` plugin, or whether bundles carry hooks too. Copilot and Codex
   make that choice hard.
5. **G14.** The bundle scheme doesn't say what is signed beyond the skills: the version, the "needs Otaq ≥" range,
   the index file. That leaves room to roll an app back to an older bundle or freeze it on one. If the key leaks,
   "the newest bundle always wins, no withdraw" means a bad bundle stays in place until a new app release.

What already holds, so it isn't repeated below:

- S9's premise checks out: 31 commits touched `plugin/skills/` since `v0.1.0`, and 29 of them also touched `crates/`.
- Claude Code and Copilot already get a `--plugin-dir` at each launch (`crates/otaq-core/src/agent_tools/claude/launch.rs:262-269`,
  `copilot/launch.rs:89`), and OpenCode gets a skills folder in each launch's config (`opencode/launch.rs:221-222`).
  So pointing a session at a folder per bundle fits these three tools' launch code (Codex is the exception, G4).
- S21's "no symlinks" matches the runtime copy, which already uses plain copies for Windows (`crates/otaq-core/src/runtime.rs:31-33`).
- S24's single fixed file with no API call matches the app updater's `latest.json` pattern (`crates/otaq-core/src/naming.rs:124-127`).
- Copilot runs skills that carry `disable-model-invocation: true` (T10 check 8, `ideas/no-skill-copies/T10-report.md:28`).
- The skill finder already reads a tool's plugin skills as `<plugin>:<skill>`, a `tuq` plugin included
  (`crates/otaq-core/tests/suite/found_skills.rs:661`).
- History already has one "Session · Dev · Strong" row per session (`crates/otaq-view/src/view_models/history.rs:315`), so
  S25's "skills 14 Nov" has a place. Settings → About already holds updates (UX-10), as S25 needs.
- The ready stages already name `security-check`, `test-in-browser` and `update-docs` (`crates/otaq-core/flows/stage-templates.yaml:23,33,42`).

---

## Blocks release

### G1. Existing flows name Otaq's skills by bare name, which S15 turns into the user's skills

- **Decisions:** S15 ("A bare name in a flow is always the user's skill"), S5, S8 (existing installs), UX-61 (ready
  stages).
- **What's wrong:** S15 changes what a bare name means, but no decision says what happens to the flows that already
  exist. Without a rule, every Dev stage that says `skill: work-ticket` names a user skill nobody has, so S5 holds
  every ticket. The same goes for a Validate stage with `skill: validate` and every delivery with `skill: open-pr`.
  Users who chose to have their own `validate` today get Otaq's (S3). After the upgrade they would quietly get their
  own.
- **Evidence:**
  - Presets: `crates/otaq-core/flows/otaq-default.yaml:11,18,30`, `spec-first.yaml:17,24,31,45,52,66`,
    `team-pr.yaml:13,20,27`, `solo-autopilot.yaml:11,18`.
  - Ready stages: `stage-templates.yaml:23,33,42,58,67`.
  - Stored flow versions are never changed once written; only agent renames touch them
    (`crates/otaq-core/src/flow/store.rs:4-7`).
  - Today, own-skill detection is a fixed list of bare names (`crates/otaq-core/src/agents/shipped.rs:22-37`), used by
    the held-stage check (`crates/otaq-core/src/board/missing_skill.rs:40,53`).
  - Flows also arrive through Import and through Noyan's `flow` skill, which writes bare names today.
- **Sure:** high.
- **Decision that would close it:** At upgrade, are the bare names of the 13 shipped skills in stored and imported
  flows read as `tuq:<name>` (one rule for old versions, or a new version written), and does a user who has their
  own skill of that name keep Otaq's, keep theirs, or get asked?

### G2. Built-in skills and bundles have no order, and a bundle has no upper bound

- **Decisions:** S10 (the built-in copy is the fallback "whenever no newer bundle is usable"), S12, S22, S23.
- **What's wrong:**
  - "Newer" is never defined between a bundle and the built-in skills. Say app 0.4 ships `plugin/tuq/` as it was at
    release, and the newest bundle is from before that release and says "needs Otaq ≥ 0.3". Under S23 that bundle
    wins, so the new app runs older skills than it shipped with, which may not know 0.4's features.
  - A bundle names only the oldest Otaq it works with. If a later app removes or renames an `otaq` command or flag,
    every earlier bundle still claims to work with it.
- **Evidence:** Skills and code change together (29 of 31 skill commits, S9). Skills call the CLI heavily: from 6
  `otaq ` calls in `answer-question` to 79 in `work-ticket` (`grep -c "otaq " plugin/skills/*/SKILL.md`).
- **Sure:** high.
- **Decision that would close it:** Is a bundle used only when it is newer than the app's own built-in skills (by
  the commit or date each was taken from), and can a bundle also state the newest Otaq it works with?

### G3. The `tuq` commands: where they come from, Windows, and permission prompts

- **Decisions:** S17 ("`tuq`, `tuq-plan` on the PATH, on macOS, Windows and Linux"), S14 (a bundle carries only
  skills), S12.
- **What's wrong:**
  - The commands are bash scripts that find their files relative to themselves. On Windows, Codex runs an agent's
    commands in PowerShell, which can't run them. `tuq-plan pull` also needs python3.
  - The decisions don't say whether the commands ship in the bundle (code updated outside the app) or in the app (a
    rewrite, so a bundle using a new subcommand needs a newer app under S12).
  - The decisions don't say which per-session PATH entry they come from (S13).
  - Every tool pre-approves only `otaq`, so each `tuq` call would stop at a permission prompt in an unattended stage.
- **Evidence:**
  - `otaq-claude-skills/plugins/tuq/bin/tuq:1,12` (bash, `PLUGIN_DIR` from `BASH_SOURCE`) and `bin/tuq-plan:7`
    (python3).
  - Codex uses PowerShell on Windows: `crates/otaq-core/src/shell.rs:12-15`, `crates/otaq-cli/src/prime.rs:118-120`.
  - Pre-approvals: Claude Code `Bash(otaq:*)` (`claude/launch.rs:262-269`), Copilot `shell(otaq:*)`
    (`copilot/launch.rs:92`), OpenCode `otaq *` (`opencode/launch.rs:218-226`), Codex `prefix_rule(pattern=["otaq"])`
    (`codex/setup.rs:27-28`).
  - `tuq/decisions/064` itself notes that today only Claude Code puts a plugin's `bin/` on the PATH.
- **Sure:** high.
- **Decision that would close it:** Do `tuq` and `tuq-plan` become part of the app (for example `otaq tuq …`, already
  pre-approved and cross-platform), or ship in each bundle with their own pre-approval on all four tools?

### G4. S13 ("a session keeps its bundle, resumes included") can't hold as the code stands

- **Decisions:** S13, S10, S8.
- **What's wrong:**
  - **Codex** runs one installed copy of Otaq's plugin, which the service refreshes in place. Sessions on different
    bundles at the same time, or a running session keeping its bundle while another stage moves to a newer one, have
    no home. S13 hands this to "the technical plan", but it may not be possible without a separate Codex home per
    session.
  - **Built-in skills** live in the build's runtime folder, and every other runtime folder is deleted at service
    start. A session started on build A's built-in skills therefore can't resume on them after an update to build B.
  - **Sessions from before this change** started with `/otaq:` skills and have no bundle at all.
- **Evidence:**
  - Codex's plugin copy is refreshed by `codex plugin add` and named by the manifest's version
    (`crates/otaq-core/src/agent_tools/codex/setup.rs:14-22`). It is the one fixed folder `runtime/codex-plugin`
    (`runtime.rs:24-26`).
  - Cleanup deletes the other runtime folders (`runtime.rs:19-23`, `fn clean` at `runtime.rs:240`).
- **Sure:** medium (Codex), high (built-in skills across an update).
- **Decision that would close it:** Do built-in skills count as a bundle that is kept while any session uses it? On
  Codex, does S13 hold, or is "Codex sessions see the newest bundle" accepted? What do sessions started before the
  upgrade resume on?

### G5. OpenCode's `tuq-<name>` copy is built only by the publish command, not by the app build

- **Decisions:** S21 (the publish command builds an OpenCode version), S9, S22 (built-in skills run when there is no
  bundle).
- **What's wrong:** A new install that is offline, a build from source, or any app before its first bundle runs the
  built-in skills. The app bundles `plugin/` exactly as it is, with no OpenCode copy. OpenCode sessions would then be
  told to load `tuq-validate`, which doesn't exist, and would stop, as S3's first message tells them to. F13's
  OpenCode end-to-end check (with `disable-model-invocation`) is still open as well.
- **Evidence:** `crates/otaq-desktop/tauri.bundle.conf.json:10` (`"../../plugin/": "plugin/"`); OpenCode's config
  points at one skills folder (`opencode/launch.rs:221-222`).
- **Sure:** high.
- **Decision that would close it:** Does the app build make the same `tuq-<name>` OpenCode copy as the publish command
  (one shared step), and does `otaq-io/tuq` carry it for people who install by hand?

## Confuses users

### G6. Users who already installed `tuq` themselves

- **Decisions:** S15, S26, S18, S6.
- **What's wrong:**
  - The tuq repo has told people to install `tuq@otaq-claude-skills` and `browser-validate@otaq-claude-skills`, and
    S26 now invites everyone to install tuq without Otaq. Such a user's Claude Code session would load two plugins
    named `tuq`: theirs and Otaq's `--plugin-dir`. Nothing in the repo shows which one Claude Code picks.
  - A flow that names `tuq:product-spec` today means the user's installed plugin. Under S15 it means Otaq's.
  - The finder hides Otaq's own namespace from the plugin list. Once that namespace is `tuq`, the user's installed
    tuq disappears from Pick a skill….
  - S26 writes `plugin/tuq/` into the renamed repository but doesn't keep the marketplace layout, so existing
    installs (`marketplace.json` → `./plugins/tuq`, `autoUpdate: true`) may break.
  - Removing `browser-validate` (S18) breaks project skills that hand off to `browser-validate:validate-in-browser`
    (the tuq repo's README cites ski-web's `.claude/skills/ski-validate-in-browser/`).
- **Evidence:**
  - The maintainer's own Claude Code has `tuq@otaq-claude-skills` enabled (`~/.claude/settings.json:18`).
  - `otaq-claude-skills/README.md` (Install, team settings); `.claude-plugin/marketplace.json`.
  - The own-namespace filter is at `crates/otaq-core/src/agent_tools/mod.rs:417`.
- **Sure:** high on the facts; medium on what Claude Code does with two plugins of the same name.
- **Decision that would close it:** When the user also has a `tuq` plugin installed, which one does an Otaq session
  use (and does Otaq warn)? Does `otaq-io/tuq` keep a marketplace at the same path, so existing installs keep
  updating?

### G7. Skills that name each other by bare name contradict "a bare name is the user's"

- **Decisions:** S21 ("skills name each other harness-neutrally (Call the Skill tool with `step-back`)"), S15, S2
  (a hand-off can always type a skill by name).
- **What's wrong:**
  - Inside a session, a bare `commit` or `step-back` can resolve to the user's own skill of that name on Claude Code,
    Codex and Copilot. The OpenCode copy is rewritten (S21), but the other three aren't.
  - S21 also gives `disable-model-invocation` to skills "only a person may start". A skill that another skill calls
    through the Skill tool (Step back from Product spec) must not carry it, and the decisions don't list which skills
    carry it.
- **Evidence:**
  - Today's skills say "Otaq's `commit` skill" (`plugin/skills/work-ticket/SKILL.md:50,567`; also in
    `address-review`, `restack`, `product-spec`, `tech-spec`, `milestone-plan`). `commit` is the only shipped skill
    without the flag (`grep disable-model-invocation plugin/skills/*/SKILL.md`).
  - tuq's Product spec runs `tuq:step-back` (`otaq-claude-skills/plugins/tuq/skills/product-spec/SKILL.md:89-90`).
- **Sure:** medium.
- **Decision that would close it:** Do skills name each other in the namespaced form (`tuq:commit`, rewritten per
  tool at publish and build), and which tuq skills are person-only?

### G8. tuq's Product spec starts itself; Otaq's doesn't

- **Decisions:** S19 (keeps tuq's "plain-chat mode"), S21, Noyan intake decisions (Noyan files tickets; it doesn't
  spec).
- **What's wrong:** tuq's `product-spec` has no `disable-model-invocation`, and its description says to use it "when
  the user starts describing or exploring a feature". Otaq's has the flag. Once the tuq plugin is in every Otaq
  session, a Noyan conversation ("I want users to…") or a Dev session could start Product spec by itself. S19 keeps
  the plain-chat mode without saying whether the flag stays.
- **Evidence:** `otaq-claude-skills/plugins/tuq/skills/product-spec/SKILL.md:2-9` (no flag) versus
  `plugin/skills/product-spec/SKILL.md:4` (`disable-model-invocation: true`).
- **Sure:** medium.
- **Decision that would close it:** Is the merged Product spec person-only (and the plain-chat mode reached by typing
  `/tuq:product-spec`), or may agents start it, and if so, not inside Noyan's or a stage's session?

- **Outcome (2026-11-14):** decision 089: inside Otaq, no skill starts by itself.

### G9. "Decisions always live in `tuq/`" for projects that keep them elsewhere

- **Decisions:** S20, S17.
- **What's wrong:**
  - Otaq's Product spec today finds where the project keeps its decisions in the project's notes, asks once if
    nothing says, and records the answer. S20 drops that. A customer project whose notes say `decisions/` or
    `docs/product.md` gets a second decisions folder, and Tech spec and Milestone plan only read `tuq/`.
  - The pointer is added to README and CLAUDE.md only. Codex, Copilot and OpenCode read AGENTS.md, so "the same on
    every tool" (S17) isn't met.
- **Evidence:** `plugin/skills/product-spec/SKILL.md:41-45` (find the place, else ask once and record it).
- **Sure:** high.
- **Decision that would close it:** Does a project that already keeps decisions somewhere else keep them there (or
  get moved once), and is the pointer also written to AGENTS.md?

### G10. "Install tuq without Otaq" on the four tools

- **Decisions:** S26 (its README tells people how to install tuq on each of the four tools), S17.
- **What's wrong:**
  - Outside Otaq, only Claude Code puts a plugin's `bin/` on the PATH. On Codex, Copilot and OpenCode, the skills hit
    "`tuq: command not found` … stop and say so".
  - Most tuq skills (`work-ticket`, `validate`, `noyan`, `commit`…) need `otaq` and `$OTAQ_TICKET` and are useless
    on their own. `commit` has no `disable-model-invocation`, so it could start in a person's ordinary sessions.
- **Evidence:**
  - `tuq/decisions/064` (Why).
  - `otaq-claude-skills/plugins/tuq/skills/product-spec/SKILL.md:25`.
  - The `otaq ` call counts per skill, as in G2.
- **Sure:** medium.
- **Decision that would close it:** Does `otaq-io/tuq` publish everything (and say which skills need Otaq), or only
  the general skills, and is a stand-alone install on Codex, Copilot and OpenCode promised to work?

## Undecided

### G11. The hooks plugin and the tuq plugin: one or two?

- **Decisions:** S15 ("Nothing says `otaq:` any more"), S13, S21 ("one `.claude-plugin/plugin.json` serves Claude
  Code, Codex and Copilot"), S2.
- **What's wrong:**
  - Each tool's plugin, named `otaq`, carries Otaq's hooks and its skills together. Its hooks differ per tool (Copilot
    passes `--tool copilot`; Codex's hooks are in its manifest).
  - For skills to be `tuq:<name>`, either a second plugin named `tuq` sits next to the `otaq` hooks plugin, or the
    hooks plugin is renamed. A rename changes Codex's hook trust keys, which are keyed by plugin id.
  - Copilot ignores skill paths outside its plugin folder (S2). If Copilot takes a single `--plugin-dir`, each bundle
    folder must be a whole Copilot plugin, hooks included, so the signed bundle would carry code that runs by itself.
- **Evidence:**
  - Plugin manifests: `plugin/claude/.claude-plugin/plugin.json` ("its agent signal hooks and Otaq's own skills"),
    `plugin/codex/.codex-plugin/plugin.json`.
  - `crates/otaq-core/src/naming.rs:22-23`; `agent_tools/mod.rs:254-257` (`PLUGINS_WITH_SKILLS`);
    `runtime.rs:27-34`.
  - Codex trust keys `otaq@otaq-local:plugin.json#hooks[0]:<event>:0:0` (`codex/setup.rs:25,82`).
- **Sure:** high that it's undecided.
- **Decision that would close it:** Do the hooks stay in an `otaq` plugin per tool with a separate `tuq` plugin for
  skills (two plugins per session), and do bundles ever carry hooks?

### G12. A `tuq:` skill the running app or bundle doesn't have

- **Decisions:** S15, S6 ("Otaq's own skills never warn"), S5, S12, S2.
- **What's wrong:**
  - Under S15 any `tuq:` name is Otaq's, so it is never checked or held. But an app on an older bundle (S12), or on
    its built-in skills (S22), may not have `tuq:update-docs`. That happens when a teammate's flow, Import or a newer
    `flow` skill names it.
  - S2's "every shipped skill is in every session, so a hand-off can always type it" stops holding once a session
    keeps an older bundle (S13) and the app types a skill added later.
- **Evidence:** `is_own_skill` short-circuits both checks (`crates/otaq-core/src/board/missing_skill.rs:40,53`). The
  list is compiled in (`agents/shipped.rs:22-37`).
- **Sure:** high.
- **Decision that would close it:** Is a `tuq:` skill missing from the session's bundle treated like any missing
  skill (held, "Otaq's skills don't have `tuq:x` yet"), and does Otaq check this for typed hand-offs too?

### G13. Where bundles live, and how one fixed file serves every app

- **Decisions:** S24 ("one fixed file on GitHub (a release asset)"), S12 ("every published bundle stays available";
  older apps take the last bundle in their range), S26.
- **What's wrong:**
  - One fixed file can serve S12 only if it is an index of all bundles and their ranges, and the decisions don't say
    so.
  - Neither the repository nor the release is named. If skill releases go into `otaq-io/otaq`, each one becomes that
    repository's "latest" release. The app updater and the website both read `releases/latest`.
- **Evidence:** `crates/otaq-desktop/tauri.conf.json:20`, `crates/otaq-core/src/naming.rs:124-127`,
  `website/src/components/links.ts:5`.
- **Sure:** high that it's undecided; medium on the clash.
- **Decision that would close it:** Are bundles released in `otaq-io/tuq`, with a fixed asset (an index of every
  bundle and its range) that an app reads to pick its own?

### G14. What the signature covers, rollback, and a leaked key

- **Decisions:** S11, S22, S23 ("no withdraw"; "the newest usable bundle always wins").
- **What's wrong:**
  - **Unsigned index.** With minisign, as the app updater uses, only the file itself is signed. If the index
    (version, range, note) isn't signed too, a mirror or a taken GitHub account can serve an older signed bundle to a
    new install, or keep apps frozen on one.
  - **Key.** S11 says "as with app updates" but doesn't say whether this is the same key. If it is, an
    app-update file and a bundle are signed alike unless the bundle says what it is.
  - **Leaked key.** A leaked key, or a mistaken far-future version, gives a bundle that "always wins". With no
    withdraw, only an app release with a new key can undo it.
  - **Contents.** The decisions don't say whether a bundle may carry executables (`bin/`, hooks, an MCP server; G3,
    G11, G16).
- **Evidence:** `crates/otaq-desktop/src/updates.rs:36-60` (the built-in minisign key and how it is checked);
  `tauri.conf.json:19-21`.
- **Sure:** medium.
- **Decision that would close it:** Does each bundle sign its own version, kind ("skills"), range and note with its
  own key, does an app never go below a bundle it has used, and is there a way out (a revocation list in the app, or a
  "stop bundles" release) if the key leaks?

### G15. `validate-in-browser`: merged into `validate`, or its own `test-in-browser`?

- **Decisions:** S16 (merged with Otaq's own "where both exist (… `validate` / `validate-in-browser`)"), S18 (becomes
  `tuq:test-in-browser`), S19 (`validate-in-browser` checked "against Otaq's versions").
- **What's wrong:** S16 reads as one merged validate skill. S18 keeps a separate browser skill. A builder can't tell
  whether `tuq:validate` gains browser steps.
- **Evidence:** the decision text; `tuq/decisions/057` lists `test-in-browser` separately and drops the merge pairing.
- **Sure:** high.
- **Decision that would close it:** Is `tuq:test-in-browser` a separate skill that takes Otaq `validate`'s later fixes
  (and `tuq:validate` has no browser part)?

### G16. Test in the browser depends on macOS Chrome, Claude-only MCP wiring and outside tools

- **Decisions:** S18, S17 (`tuq/decisions/064`: "No tuq skill is Claude-only or macOS-only"), S14 (a bundle carries
  only skills).
- **What's wrong:**
  - The skill drives `/Applications/Google Chrome.app` only.
  - It watches through an MCP server started from a Claude Code plugin's `.mcp.json` (`${CLAUDE_PLUGIN_ROOT}`), and
    calls it by a Claude-only tool name.
  - It needs `playwright-cli` and `npx`.
  - The decisions don't say whether the MCP server and scripts travel in the bundle, or how Codex, Copilot and
    OpenCode get the server.
- **Evidence:**
  - `otaq-claude-skills/plugins/browser-validate/scripts/browser.sh:16,93`;
    `.mcp.json`; `scripts/devtools-mcp.sh:12` (`npx --yes chrome-devtools-mcp@1.12.0`).
  - `skills/validate-in-browser/SKILL.md:152` (`mcp__plugin_browser-validate_browser-watch__list_pages`).
  - The tuq repo's README: "needs macOS Google Chrome and `playwright-cli`".
- **Sure:** high.
- **Decision that would close it:** Is Test in the browser exempt from S17 until it is ported (shown only where it
  can run), or must it work on every tool and OS before release?

### G17. How the publish command knows "needs Otaq ≥"

- **Decisions:** S12 ("which it can tell from the commit the skills come from").
- **What's wrong:**
  - Nothing says how a commit shows that a skill started using something new in `otaq`. Most skill commits also
    change code (S9), and the version that code ships in isn't known when the bundle is published.
  - A minimum set too low makes skills call commands the app lacks. S22 doesn't cover that: such a bundle is
    "usable".
- **Evidence:** 29 of 31 skill commits since `v0.1.0` also changed `crates/` (counted with `git log`).
- **Sure:** medium.
- **Decision that would close it:** Is the minimum set to the first released version that contains every commit up
  to the bundle's commit (no guessing), or does the maintainer confirm it at publish?

### G18. Builds from source, tests and the `.deb`

- **Decisions:** S10, S24.
- **What's wrong:**
  - App updates are off in builds from source and before the app is installed where it can update, and the `.deb`
    only reports a new version. The decisions don't say whether the skill check follows the same gates.
  - A dev build that fetches bundles would run published skills instead of the `plugin/tuq/` being edited.
  - Tests need a test endpoint and key, as the updater has.
- **Evidence:** `crates/otaq-desktop/src/updates.rs:9-16` (`UPDATES_TEST_ENDPOINT` / `UPDATES_TEST_PUBKEY`, "never in
  a build from source").
- **Sure:** high.
- **Decision that would close it:** Do builds from source never fetch bundles (only a test override), while every
  release build fetches them, the `.deb` included?

### G19. Codex and "one `.claude-plugin/plugin.json`"

- **Decisions:** S21, S2 (Codex's installed plugin), F16.
- **What's wrong:**
  - Otaq's Codex setup checks for `.codex-plugin/plugin.json`, and names Codex's cached copy from that file's
    `version`. Nothing in the repo shows that Codex reads `.claude-plugin/plugin.json`.
  - A second Codex plugin (`tuq@…`) also needs adding to setup, refresh and disconnect, and S8's clean-up must remove
    the skills left in the old `otaq@otaq-local` copy, so Codex doesn't list both `$otaq:validate` and
    `$tuq:validate`.
- **Evidence:** `crates/otaq-core/src/agent_tools/codex/setup.rs:247-251,481-485`.
- **Sure:** low on the manifest; medium on the rest.
- **Decision that would close it:** Is the Codex part of S21 confirmed hands-on before it is built, and does
  disconnecting Codex remove the tuq plugin too?

### G20. What `security-check` and `update-docs` do

- **Decisions:** S16 ("the new `security-check` and `update-docs`").
- **What's wrong:** Both are named but have no description, stage behaviour or checks to meet. The ready stages
  already offer them, and today a ticket that reaches them is held (`tuq/decisions/065`, Why).
- **Evidence:** `crates/otaq-core/flows/stage-templates.yaml:17-24,37-45`; no such skill in either repository.
- **Sure:** medium.
- **Decision that would close it:** Are these two specced (Product spec) before the release that brings tuq, or
  left out of the ready stages until they are?

## Cosmetic

### G21. Decision 015 versus "on screen `tuq:validate`"

- **Decisions:** S15, S21 ("`tuq:validate` is Otaq's notation in flow files and on screen"), `tuq/decisions/015` (a
  skill in a message about a stage uses that stage's tool form, `$name` for Codex).
- **What's wrong:** The two rules disagree for messages about a stage: "Codex doesn't have the skill …" would use
  `tuq:validate` under S21 and `$tuq:validate` under 015.
- **Evidence:** `tuq/decisions/015-skill-named-as-tool-calls-it.md`.
- **Sure:** medium.
- **Decision that would close it:** Does 015 still apply to stage messages (tool form), with `tuq:` everywhere else?

### G22. Docs and website still show `/otaq:` and bare names

- **Decisions:** S15, S26.
- **What's wrong:** The docs and the website name skills the old way, and the decisions don't list them for
  updating.
- **Evidence:**
  - `docs/how-it-works.md:135-145` (`/otaq:<name>` table) and `docs/otaq-command.md:68-75`.
  - `README.md:32-33`.
  - `website/src/components/Flow.astro:57-72` and `website/src/demo/data.ts:105-118` (`/work-ticket`, `/validate`).
- **Sure:** high.
- **Decision that would close it:** none needed beyond listing them in the plan.

- **Outcome (2026-11-14):** agreed to update the docs, README and website at the end of this work.

### G23. Two records of the same decisions, one incomplete

- **Decisions:** S9–S26 and `tuq/decisions/057`–`073`.
- **What's wrong:** `ideas/no-skill-copies/DECISIONS.md` says it is the record. The 17 tuq decisions copy it but
  leave out S16's merge sentence and most of S21: the namespace rule, harness-neutral references, the `openai.yaml`
  policy, one manifest, no symlinks. Only S21's OpenCode part became 072.
- **Evidence:** `tuq/decisions/057` ("Was S9 and S16"), `072` ("Part of S21"); `CLAUDE.md` (this round's decisions
  are in `ideas/`).
- **Sure:** high.
- **Decision that would close it:** Which file is the record, and does the rest of S21 get its own tuq decision?

### G24. About and History labels when no bundle is in use, or two the same day

- **Decisions:** S25.
- **What's wrong:**
  - The decisions don't say what About and History show for built-in skills, which have no publish date or note.
  - Two bundles published the same day both read "skills 14 Nov".
- **Evidence:** the decision text.
- **Sure:** medium.
- **Decision that would close it:** Do built-in skills show as "skills built in · <app version>", and does the label
  add a time or number when two bundles share a day?
