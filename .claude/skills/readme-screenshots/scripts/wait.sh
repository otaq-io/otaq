#!/bin/zsh
# wait.sh <status> <KEY>... [-t seconds]: wait until every ticket's status in `otaq list` starts with <status>
# ("Ready for Review", "Working", "Merged", ...). Prints the board's list when done. Exits 1 on timeout (default 600 s),
# or early when a ticket shows a status that needs a hand (Start failed, Resume failed, Asks you, Needs you).
DEMO=${OTAQ_DEMO:-/tmp/otaqdemo}
REPO=$(git -C ${0:A:h} rev-parse --show-toplevel)
export OTAQ_HOME=$DEMO/home
want=$1; shift
timeout=600; keys=()
while (( $# > 0 )); do
  if [[ $1 == -t ]]; then timeout=$2; shift 2; else keys+=$1; shift; fi
done
end=$((SECONDS + timeout))
while (( SECONDS < end )); do
  list=$($REPO/target/debug/otaq list -P NOTE)
  done_all=1
  for k in $keys; do
    st=$(print -r -- $list | awk -v k=$k '$1 == k { $1=""; $2=""; sub(/^ +/, ""); print }')
    if [[ $st == (Start failed|Start interrupted|Resume failed|Asks you|Needs you|Idle)* ]]; then
      print -r -- $list; echo "$k needs a hand: $st" >&2; exit 1
    fi
    [[ $st == $want* ]] || done_all=0
  done
  (( done_all )) && { print -r -- $list; exit 0; }
  sleep 10
done
print -r -- $list; echo "timeout after ${timeout}s" >&2; exit 1
