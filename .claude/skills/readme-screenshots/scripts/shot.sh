#!/bin/zsh
# shot.sh <out.png> [step]...
# Brings the Otaq window to the front, runs the steps, moves the pointer off the window (a hovered card dims the
# rest of the board) and captures the window to <out.png>. Steps, in window-relative points (the same pixels as an
# earlier shot of the window on a 1x display):
#   <x> <y>                    click there, then wait 1.2 s
#   scroll <x> <y> <lines>     scroll there (negative lines scroll down)
#   wait <seconds>             wait
# With no steps it only captures. It warns when the window isn't the expected 1440x900, which happens when macOS
# shows it scaled (Mission Control, a space switch): capture again.
set -e
DEMO=${OTAQ_DEMO:-/tmp/otaqdemo}
HERE=${0:A:h}
mkdir -p $DEMO/bin
for t in mouse window; do
  [[ $DEMO/bin/$t -nt $HERE/$t.swift ]] || swiftc -O $HERE/$t.swift -o $DEMO/bin/$t
done

out=$1; shift
REPO=$(git -C $HERE rev-parse --show-toplevel)
# Only the demo app's window: an installed Otaq that is open too is also named otaq-app, and is never captured.
export OTAQ_APP_PID=$(pgrep -f "^$REPO/target/debug/otaq-app" | head -1)
[[ -n $OTAQ_APP_PID ]] || { echo "the demo app isn't running (start.sh)" >&2; exit 1; }
read id X Y W H < <($DEMO/bin/window) || { echo "no Otaq window found" >&2; exit 1; }
# Bring the demo app forward by its pid, with no click: a click on the window (even its header's edge) would close an
# open dialog or menu, and the first click on an inactive window only activates it.
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $OTAQ_APP_PID) to true" \
  >/dev/null || { echo "can't bring the demo app forward (System Events needs Accessibility for this terminal)" >&2; exit 1; }
sleep 0.4
while (( $# > 0 )); do
  case $1 in
    scroll) $DEMO/bin/mouse $((X + $2)) $((Y + $3)) move $4; sleep 0.8; shift 4 ;;
    wait)   sleep $2; shift 2 ;;
    *)      $DEMO/bin/mouse $((X + $1)) $((Y + $2)) click; sleep 1.2; shift 2 ;;
  esac
done
$DEMO/bin/mouse $((X + W / 2)) $((Y + H + 150))
sleep 0.6
screencapture -x -o -l $id $out
size=$(sips -g pixelWidth -g pixelHeight $out | awk '/pixel/ {printf "%s%s", sep, $2; sep="x"}')
echo "$out  (window $id at $X,$Y ${W}x$H, image $size)"
[[ $size == ${W}x$H && $W == 1440 ]] || echo "WARNING: unexpected size; look at the shot and capture again if it's scaled" >&2
