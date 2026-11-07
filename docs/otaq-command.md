# The `otaq` command

[← README](../README.md)

`otaq` is both your tool and the agents' tool. Agents use it (taught by Otaq's bundled skills) to read their ticket,
ask questions and submit; you can use the same commands from any terminal.

## Your own terminal

Agents don't need anything set up. To run `otaq` in your own terminal, open **Settings → About** and click **Add to
my shell's PATH** under "Use otaq in your own terminal". Nothing is edited until you click. It adds one line to your
login shell's file, marked `# added by Otaq`:

- zsh: `~/.zshrc` (or `$ZDOTDIR/.zshrc`); bash: `~/.bashrc`, and on macOS also `~/.bash_profile` when that doesn't
  read `~/.bashrc`; fish: `~/.config/fish/conf.d/otaq.fish`;
- Windows: `%USERPROFILE%\.otaq\bin` is added to your account's PATH (no PowerShell profile is edited).

The line is `export PATH="$HOME/.otaq/bin:$PATH" # added by Otaq` (fish: `set -gx PATH "$HOME/.otaq/bin" $PATH`).
Open a new terminal afterwards. If the file can't be written, About shows the line to paste yourself, with Copy.
`~/.otaq/bin/otaq` always points at the Otaq you opened last.

```sh
otaq                         # open the app
otaq list -P SHOP            # the project's tickets
otaq create "Fix login" -p 2 --deps SHOP-12
otaq create "Fix README typo" --stage Dev=Light  # another agent for one stage
otaq create -f plan.md       # many tickets from one Markdown file
otaq create "Wishlist" --plan-file wishlist-plan.md   # one ticket with a Plan of pieces
otaq update SHOP-40 --plan-file wishlist-plan.md      # replace its Plan (or --plan, --plan-stdin)
otaq show SHOP-18
otaq show SHOP-40 --piece 2  # one piece of the Plan
otaq delete SHOP-41 --reason "folded into SHOP-40"   # a never-started ticket you created
otaq dep tree SHOP-18
otaq attach SHOP-18          # this ticket's live agent session, in your terminal
otaq daemon status           # the background service
otaq uninstall               # remove what Otaq left on this computer (asks first; see below)
```

The agent in charge of a ticket runs its helpers with `otaq helper`:

```sh
otaq helper start 2 --title "Wishlist button" --stdin   # piece 2, with a note on stdin
otaq helper list             # each helper's piece, status and branch
otaq helper diff 2           # what merging it would bring
otaq helper back 2 --stdin   # send it back with a note
otaq helper merge 2          # merge it into the ticket's branch
otaq helper drop 2 --stdin   # drop the piece, with the reason: its helper stops, its branch goes
otaq helper answer 2 --stdin # answer its question
otaq helper pass-on <id> --note "a product question the Plan doesn't answer"
otaq helper done --stdin     # a helper hands its piece back, with a summary
```

A ticket's agents use its [Run](how-it-works.md#run-and-terminal) with `otaq run` (with several apps, each command names one):

```sh
otaq run start --wait        # start the ticket's Run, or use the one already up; prints its URL
otaq run start api --wait    # one app of a monorepo
otaq run status              # each app: not started, running :4020 (started by Dev), stopped
otaq run log --tail 50       # its last lines (kept in memory only)
otaq run restart --wait      # serve changed code: same command and port, same owner
otaq run stop                # stop a Run an agent started (yours keeps running)
otaq run set --command 'pnpm dev --port $PORT'   # add a missing run command (or app)
```

Every command takes `--json`. Run `otaq --help` for the full list.

The agent's workflow lives in [`plugin/`](../plugin) as skills: `work-ticket` (the main loop, for the agent in charge
and for its helpers), `validate`, `commit`, `push`, `open-pr`, `address-review`, `answer-question`, `restack`, `flow`,
`product-spec`, `tech-spec`, `milestone-plan` (the Spec first preset's planning stages) and `noyan` (files tickets
from a request, with notes for the next stage). They come with Otaq's plugin for each tool, the same one that carries
its hooks: `--plugin-dir` for Claude Code and Copilot, the `otaq@otaq-local` plugin for Codex, and a `skills` entry in
OpenCode's config. So every session has all of them from its start. To run a skill of your own instead, name it on the
flow (your own Validate skill on the Validate stage, your own Open PR skill as the `skill:` of the flow's `delivery:`); see
[Skills](how-it-works.md#skills). The hooks only act when `OTAQ_TICKET` is set, so your ordinary sessions are
unaffected.

