# 01 · The restart dialog calls an installed, signed-in tool "not installed"

**Kind:** bug · **Release:** must fix · **From:** re-test FAIL, piece 4 / failure 24 (NEW-1)

## What happens
Right after a restart, the "agents were stopped by a restart" dialog says an installed, signed-in tool isn't
available. It leaves the agent unticked, and the button reads "Resume 0" for an agent that could resume.
- First start of the new build: "SHOP-10 … Codex isn't available (not installed)", unticked, "Resume 0". A few
  seconds later Settings → Agents said Codex "Connected · signed in with ChatGPT", and SHOP-10's own Resume worked.
- Controlled restart with SHOP-18 working on a Copilot agent: "Copilot isn't available (not installed)", unchanged after
  20 s, while Settings → Agents in the same session showed "GitHub Copilot · Connected · v1.0.108". (That launch had
  Codex, not Copilot, removed from PATH.)
- A plain Codex restart with nothing removed from PATH worked: ticked row, "Resume 1".

## Evidence
`../shots-retest/p04-first-start-codex-not-installed.jpg`, `p04-copilot-not-installed.jpg`,
`p04-settings-copilot-connected.jpg`, `p04-resumable-row.jpg`; RESULTS-retest.md, piece 4.

## Where to start
`crates/otaq-core/src/agent_tools/connected.rs` (`NotInstalled`, the "isn't available · not installed" text) and the
restart dialog's rows from piece 4. Open question: where does the dialog get the tool's state? It may read it before
the service's first tool check has finished, or from the wrong tool.
