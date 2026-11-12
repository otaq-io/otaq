# 18 · Flaky test under load: worktree_skills held_until_check_again

**Kind:** test flake · **From:** `scripts/check.sh` on 7f36bd79

`worktree_skills::an_agent_stage_whose_tool_lacks_its_skill_is_held_until_check_again` failed once at line 716 (the
`check_again` after a git push) during the full `cargo nextest run --workspace`. It passed alone 4 times, and the
re-run of test + doctest passed all 3411 tests. It probably waits on timing; make it wait on the state instead.
(`client_start::restart_replaces_the_service_process` in otaq-cli also failed once under load and passed alone.)
