#!/bin/sh
# Otaq's stand-in agent tool (T-0487, d0118): reads its skill folders only at start, as the real tools do, so a skill
# copied while it runs is one it doesn't know. See README.md next to it.

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

# The skills it can run by name: the copies in its folder now, and only now.
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

# One prompt: a skill by name runs when it was loaded at start and is an unknown command otherwise; "Follow the skill
# in `<file>`" runs when the file is there (read now, as any file); other text is ignored.
handle() {
    line=$1
    esc=$(printf '\033')
    cr=$(printf '\r')
    line=${line#"$esc[200~"}
    line=${line%"$cr"}
    line=${line%"$esc[201~"}
    case "$form:$line" in
    *:"Follow the skill in \`"*)
        file=${line#"Follow the skill in \`"}
        file=${file%%\`*}
        if [ -f "$file" ]; then
            printf 'FOLLOWS %s\n' "$file"
            hook UserPromptSubmit
            hook Stop
        else
            printf 'No such file: %s\n' "$file"
        fi
        return
        ;;
    slash:/*) command=${line%% *} name=${command#/} ;;
    dollar:\$*) command=${line%% *} name=${command#\$} ;;
    bare:otaq-*) command=${line%% *} name=$command ;;
    *) return ;;
    esac
    case "$loaded" in
    *" $name "*)
        printf 'RUNS %s\n' "$name"
        hook UserPromptSubmit
        hook Stop
        ;;
    *) printf 'Unknown command: %s\n' "$command" ;;
    esac
}

# The first prompt is the last argument, when it names one of Otaq's skills or a skill's file.
for last; do :; done
case "${last-}" in
/otaq-* | \$otaq-* | otaq-* | "Follow the skill in"*) handle "$last" ;;
esac

while IFS= read -r typed; do
    handle "$typed"
done
