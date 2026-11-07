#!/bin/sh
# Otaq's stand-in agent tool (T-0487, d0118, T2): Otaq's own skills come from Otaq's plugin, so every one of them runs
# by its namespaced name; the user's project skills are read from its folder only at start, as the real tools do. See
# README.md next to it.

tool=${OTAQ_STAND_IN_TOOL:-$(basename "$0")}
case "$tool" in
claude) root=.claude/skills form=slash ;;
copilot) root=.agents/skills form=slash ;;
codex) root=.agents/skills form=dollar ;;
opencode)
    root=.agents/skills form=bare
    # OpenCode's conversation is made before the launch (`opencode api --standalone POST /api/session …`).
    case "$1" in api) exit 0 ;; esac
    ;;
*)
    echo "stand-in: no tool named $tool" >&2
    exit 2
    ;;
esac

# Otaq's own skills, which Otaq's plugin gives every session from its start (`plugin/skills/`).
own=" address-review answer-question commit flow milestone-plan noyan open-pr product-spec push restack tech-spec \
validate work-ticket "

# The user's skills it can run by name: the project skills in its folder now, and only now.
loaded=" "
for dir in "$root"/*/; do
    [ -f "${dir}SKILL.md" ] && loaded="$loaded$(basename "$dir") "
done
printf 'STAND-IN %s loaded:%s\n' "$tool" "$loaded"
printf 'ARGS %s\n' "$*"

# Claude Code's hooks through `otaq hook`, only when asked (OTAQ_STAND_IN_HOOKS=1) and only as `claude`.
hook() {
    [ -n "$OTAQ_STAND_IN_HOOKS" ] && [ "$tool" = claude ] || return 0
    case "$1" in
    UserPromptSubmit) payload='{"hook_event_name":"UserPromptSubmit","prompt":"stand-in"}' ;;
    Stop) payload='{"hook_event_name":"Stop","stop_hook_active":false,"background_tasks":[],"session_crons":[]}' ;;
    esac
    printf '%s' "$payload" | otaq hook --tool claude "$1"
}

# One prompt: Otaq's skill by its namespaced name (`/otaq:x`, `$otaq:x`; OpenCode's plain `x` in its "Load the skill
# `x` …" instruction) always runs; the user's skill by its own name (`/x`, `$x`, `x`) runs when it was loaded at start
# and is an unknown command otherwise (OpenCode's model says it has no skill named `x`); other text is ignored.
handle() {
    line=$1
    esc=$(printf '\033')
    cr=$(printf '\r')
    line=${line#"$esc[200~"}
    line=${line%"$cr"}
    line=${line%"$esc[201~"}
    case "$form:$line" in
    slash:/*) command=${line%% *} name=${command#/} ;;
    dollar:\$*) command=${line%% *} name=${command#\$} ;;
    # OpenCode's: "Load the skill `x` with your skill tool and follow it. …", the name between the backticks.
    bare:"Load the skill \`"*)
        name=${line#"Load the skill \`"}
        name=${name%%\`*}
        command=$name
        ;;
    bare:*) return ;;
    *) return ;;
    esac
    known=
    case "$name" in
    */* | "") return ;;
    otaq:*)
        [ "$form" = bare ] && return
        case "$own" in *" ${name#otaq:} "*) known=yes ;; esac
        ;;
    *)
        case "$loaded" in *" $name "*) known=yes ;; esac
        if [ "$form" = bare ] && [ -z "$known" ]; then
            # OpenCode names Otaq's skills by their own names.
            case "$own" in *" $name "*) known=yes ;; esac
        fi
        ;;
    esac
    if [ -n "$known" ]; then
        printf 'RUNS %s\n' "$name"
        hook UserPromptSubmit
        hook Stop
    elif [ "$form" = bare ]; then
        printf 'No skill named %s.\n' "$name"
    else
        printf 'Unknown command: %s\n' "$command"
    fi
}

# The first prompt is the last argument, when it names a skill.
for last; do :; done
handle "${last-}"

while IFS= read -r typed; do
    handle "$typed"
done
