#!/bin/zsh
# start.sh: build the app and the otaq command from this checkout, start the demo's background service with a
# clean environment, seed the demo (first run only), start the Vite page server and open the app. Run it with the
# sandbox off (it starts processes that must outlive this shell and opens a window). Logs go to $OTAQ_DEMO.
set -e
DEMO=${OTAQ_DEMO:-/tmp/otaqdemo}
REPO=$(git -C ${0:A:h} rev-parse --show-toplevel)
export OTAQ_HOME=$DEMO/home
mkdir -p $OTAQ_HOME

# The socket path must stay under ~104 bytes (SUN_LEN): keep OTAQ_DEMO short, like /tmp/otaqdemo.
(( ${#OTAQ_HOME} < 80 )) || { echo "OTAQ_DEMO is too long for a Unix socket path: $DEMO" >&2; exit 1; }

cd $REPO
# Build both from the same tree: a service built at another time makes the app show "Service update ready".
cargo build -p otaq-cli -p otaq-desktop 2>&1 | tail -1

$REPO/target/debug/otaq daemon stop >/dev/null 2>&1 || true
sleep 1
# env -i: a service started from inside a Claude Code session would pass CLAUDE_CODE_CHILD_SESSION and friends to
# every agent, which then saves no transcript (Resume fails with "conversation not found", tokens show unknown).
env -i HOME=$HOME USER=$USER PATH=$PATH SHELL=$SHELL LANG=en_US.UTF-8 TERM=xterm-256color OTAQ_HOME=$OTAQ_HOME \
  $REPO/target/debug/otaq daemon start

# Seed before the window opens: the app decides between onboarding and the board once, at launch.
[[ -e $DEMO/pocket-notes ]] || zsh ${0:A:h}/seed.sh

# Not `npm run tauri dev`: its watcher rebuilds and restarts the app whenever any file in the repo changes
# (another session committing is enough), which loses the window mid-shoot.
pkill -f "$REPO/target/debug/otaq-app" 2>/dev/null || true
if ! curl -s -o /dev/null http://localhost:5199/; then
  (cd crates/otaq-desktop && nohup npm run dev > $DEMO/vite.log 2>&1 &)
  for i in {1..30}; do curl -s -o /dev/null http://localhost:5199/ && break; sleep 1; done
fi
nohup env -i HOME=$HOME USER=$USER PATH=$PATH LANG=en_US.UTF-8 OTAQ_HOME=$OTAQ_HOME \
  $REPO/target/debug/otaq-app > $DEMO/app.log 2>&1 &
# Only the demo app's window counts: an installed Otaq that is open too is also named otaq-app.
export OTAQ_APP_PID=$!

HERE=${0:A:h}
mkdir -p $DEMO/bin
[[ $DEMO/bin/window -nt $HERE/window.swift ]] || swiftc -O $HERE/window.swift -o $DEMO/bin/window
for i in {1..60}; do $DEMO/bin/window >/dev/null && break; sleep 1; done
sleep 4
echo "app window: $($DEMO/bin/window)"
