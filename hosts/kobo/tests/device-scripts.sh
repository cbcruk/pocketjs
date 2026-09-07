#!/bin/sh
# Exercise hosts/kobo/device against a staged root and stubbed firmware tools.
#
#   hosts/kobo/tests/device-scripts.sh
#
# The launcher's contract is that the Kobo UI comes back on every exit path.
# That is the one thing a first contact with real hardware must not get wrong,
# and it is checkable without a Kobo: point NICKEL_ROOT at a staged tree, put
# stub pidof/killall/usleep on PATH, and drive the real scripts.

set -u

DEVICE_DIR="$(cd "$(dirname "$0")/../device" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

FAILURES=0

check() {
    if [ "$2" = "$3" ]; then
        echo "ok   $1"
    else
        echo "FAIL $1: expected [$2], got [$3]"
        FAILURES=$((FAILURES + 1))
    fi
}

contains() {
    if grep -q -- "$2" "$3" 2>/dev/null; then
        echo "ok   $1"
    else
        echo "FAIL $1: [$2] not found in $3"
        FAILURES=$((FAILURES + 1))
    fi
}

NICKEL_ROOT="$WORK/root"
NICKEL_STATE="$WORK/nickel.state"
NICKEL_ENV_SEEN="$WORK/nickel.env"
NICKEL_ENV_CACHE="$WORK/nickel.env.cache"
export NICKEL_ROOT NICKEL_STATE NICKEL_ENV_SEEN NICKEL_ENV_CACHE
mkdir -p "$NICKEL_ROOT/tmp" "$NICKEL_ROOT/usr/local/Kobo" "$NICKEL_ROOT/etc/init.d"

mkdir -p "$WORK/bin"
cat >"$WORK/bin/pidof" <<'STUB'
#!/bin/sh
[ "$(cat "$NICKEL_STATE")" = running ] || exit 1
echo 4242
STUB
cat >"$WORK/bin/killall" <<'STUB'
#!/bin/sh
echo stopped >"$NICKEL_STATE"
STUB
cat >"$WORK/bin/usleep" <<'STUB'
#!/bin/sh
exit 0
STUB
# The launcher flushes the card so a freeze leaves a log; the suite must not
# flush the machine it runs on.
cat >"$WORK/bin/sync" <<'STUB'
#!/bin/sh
echo flush >>"$SYNC_LOG"
STUB
chmod +x "$WORK/bin/sync"
SYNC_LOG="$WORK/syncs"
: >"$SYNC_LOG"
POCKETJS_SYNC_SECS=1
export POCKETJS_SYNC_SECS SYNC_LOG
cat >"$NICKEL_ROOT/usr/local/Kobo/nickel" <<'STUB'
#!/bin/sh
echo running >"$NICKEL_STATE"
# Record what the restart handed us, so the test can check the UI is not
# brought back with the empty environment a telnet shell would give it.
env >"$NICKEL_ENV_SEEN"
STUB
cat >"$WORK/bin/ntx_hwconfig" <<'STUB'
#!/bin/sh
echo mx50
STUB
cat >"$WORK/bin/kobo_config.sh" <<'STUB'
#!/bin/sh
echo kraken
STUB
chmod +x "$WORK/bin/ntx_hwconfig" "$WORK/bin/kobo_config.sh"
chmod +x "$WORK/bin/pidof" "$WORK/bin/killall" "$WORK/bin/usleep" \
    "$NICKEL_ROOT/usr/local/Kobo/nickel"
PATH="$WORK/bin:$PATH"
export PATH

APP="$WORK/app"
mkdir -p "$APP/bin"
cat >"$APP/pocketjs-kobo" <<'STUB'
#!/bin/sh
echo "host args: $*"
exit "${HOST_EXIT:-0}"
STUB
chmod +x "$APP/pocketjs-kobo"
: >"$APP/app.js"
: >"$APP/app.pak"

POCKETJS_DIR="$APP"
POCKETJS_LOCK="$WORK/pocketjs.lock"
# The launcher sends its own output here when stdout is not a terminal, which
# is exactly the boot case, so that is where its refusals are.
POCKETJS_BOOT_LOG="$WORK/boot.log"
export POCKETJS_DIR POCKETJS_LOCK POCKETJS_BOOT_LOG

await_nickel() {
    # nickel is restarted in the background; give it a moment to land.
    ticks=0
    while [ "$(cat "$NICKEL_STATE")" != running ] && [ "$ticks" -lt 40 ]; do
        sleep 0.05
        ticks=$((ticks + 1))
    done
    cat "$NICKEL_STATE"
}

launch() {
    "$DEVICE_DIR/pocketjs.sh" "$@" >"$WORK/out" 2>&1
    echo $?
}

echo "-- nickel.sh --"
echo running >"$NICKEL_STATE"
check "status reports a running UI" \
    "nickel: running (pid 4242)" "$("$DEVICE_DIR/nickel.sh" status)"
check "stop halts the UI" "nickel: stopped" "$("$DEVICE_DIR/nickel.sh" stop)"
check "stop is idempotent" "nickel: already stopped" "$("$DEVICE_DIR/nickel.sh" stop)"
"$DEVICE_DIR/nickel.sh" start >/dev/null
check "start brings the UI back" "running" "$(await_nickel)"
check "an unknown subcommand is rejected" "2" \
    "$("$DEVICE_DIR/nickel.sh" bogus 2>/dev/null; echo $?)"

# rcS exports these before it starts nickel; a shell that arrived over telnet
# has none of them, and a restart without WIFI_MODULE_PATH cannot reload the
# radio — which is how a session loses its own network.
for key in PLATFORM INTERFACE WIFI_MODULE WIFI_MODULE_PATH NICKEL_HOME; do
    contains "the restart exports $key" "^$key=." "$NICKEL_ENV_SEEN"
done
check "WIFI_MODULE_PATH names a real platform" "ok" \
    "$(grep -q '^WIFI_MODULE_PATH=.*/drivers/mx50-ntx/wifi/dhd.ko$' "$NICKEL_ENV_SEEN" &&
        echo ok || echo "$(grep '^WIFI_MODULE_PATH=' "$NICKEL_ENV_SEEN")")"

echo "-- nickel.sh keeps the network --"
# dhcpcd deconfigures its interface on SIGTERM, so stopping the UI must not
# touch it: that is how a session drops off the network it arrived on.
echo running >"$NICKEL_STATE"
: >"$WORK/killed"
cat >"$WORK/bin/killall" <<'STUB'
#!/bin/sh
for arg in "$@"; do
    case "$arg" in
        -*) ;;
        *) echo "$arg" >>"$KILLED_LOG" ;;
    esac
done
echo stopped >"$NICKEL_STATE"
STUB
chmod +x "$WORK/bin/killall"
KILLED_LOG="$WORK/killed"
export KILLED_LOG
"$DEVICE_DIR/nickel.sh" stop >/dev/null
check "stopping the UI spares dhcpcd" "spared" \
    "$(grep -qx dhcpcd "$KILLED_LOG" && echo killed || echo spared)"
contains "stopping the UI still kills nickel" "^nickel$" "$KILLED_LOG"
: >"$KILLED_LOG"
"$DEVICE_DIR/nickel.sh" start >/dev/null
check "restarting hands dhcpcd back to nickel" "killed" \
    "$(grep -qx dhcpcd "$KILLED_LOG" && echo killed || echo spared)"
await_nickel >/dev/null

echo "-- remote access is opt-in --"
# A root shell with no password should never appear because a device booted.
cat >"$WORK/bin/telnetd" <<'STUB'
#!/bin/sh
echo telnetd >>"$WORK_MARKER"
STUB
chmod +x "$WORK/bin/telnetd"
WORK_MARKER="$WORK/remote.marker"
export WORK_MARKER
: >"$WORK_MARKER"
echo running >"$NICKEL_STATE"
launch >/dev/null
check "no REMOTE file means no telnetd" "quiet" \
    "$([ -s "$WORK_MARKER" ] && echo started || echo quiet)"
await_nickel >/dev/null

echo "-- pocketjs.sh --"
echo running >"$NICKEL_STATE"
check "a clean session exits 0" "0" "$(launch --present-hz 20)"
check "the UI is restored after a clean session" "running" "$(await_nickel)"
contains "host options are passed through" "--present-hz 20" "$APP/pocketjs.log"
check "the lock is released" "gone" \
    "$([ -d "$POCKETJS_LOCK" ] && echo held || echo gone)"

echo running >"$NICKEL_STATE"
check "a host failure is propagated" "3" "$(HOST_EXIT=3 launch)"
check "the UI is restored after a host failure" "running" "$(await_nickel)"

echo running >"$NICKEL_STATE"
mkdir -p "$POCKETJS_LOCK"
echo $$ >"$POCKETJS_LOCK/pid"
check "a second instance is refused" "1" "$(launch)"
contains "the refusal names the holder" "already running as pid" "$POCKETJS_BOOT_LOG"
check "the refusal leaves the UI up" "running" "$(cat "$NICKEL_STATE")"
rm -rf "$POCKETJS_LOCK"

echo running >"$NICKEL_STATE"
mkdir -p "$POCKETJS_LOCK"
echo 999999 >"$POCKETJS_LOCK/pid"
check "a stale lock is cleared" "0" "$(launch)"

# The panel update is an ioctl now, so a missing bundle is the earliest thing
# that can fail. What matters is unchanged: refuse before touching the UI.
echo running >"$NICKEL_STATE"
mv "$APP/app.pak" "$WORK/pak.hidden"
check "a missing pak fails the launch" "1" "$(launch)"
contains "the failure explains itself" "pak not readable" "$POCKETJS_BOOT_LOG"
check "the UI is never stopped when the launch fails early" "running" \
    "$(cat "$NICKEL_STATE")"
contains "a boot with no terminal still leaves a log" "launcher starting" \
    "$POCKETJS_BOOT_LOG"

rm -f "$POCKETJS_BOOT_LOG"
POCKETJS_STDOUT=/dev/console launch >/dev/null 2>&1
contains "a boot on the console still leaves a log" "launcher starting" \
    "$POCKETJS_BOOT_LOG"

# The one that bites. init gives rcS the console, which is a tty, so `[ -t 1 ]`
# read a boot as "someone is watching" and wrote no log on exactly the run
# whose output nobody could see. A pty is the only terminal that means anyone
# is; this check fails if the rule goes back to asking whether fd 1 is a tty.
rm -f "$POCKETJS_BOOT_LOG"
POCKETJS_STDOUT=/dev/pts/3 launch >/dev/null 2>&1
check "a session on a pty is left to print" "0" \
    "$([ -f "$POCKETJS_BOOT_LOG" ] && echo 1 || echo 0)"
# POSIX keeps a `VAR=x func` assignment after the function returns, unlike a
# `VAR=x command` one. Leaving it set makes every later launch think a person
# is watching, and every later boot-log check silently vacuous.
unset POCKETJS_STDOUT

mv "$WORK/pak.hidden" "$APP/app.pak"

echo "-- the hardware-status pipe --"
# rcS creates this pipe and the firmware's own event scripts write a line to
# it on every DHCP lease and every USB or SD change. Nickel is the reader, so
# with nickel gone the writer blocks in the kernel and never comes back.
STATUS_FIFO="$WORK/nickel-hardware-status"
mkfifo "$STATUS_FIFO"
export STATUS_FIFO
cat >"$APP/pocketjs-kobo" <<'STUB'
#!/bin/sh
echo "network bound 10.0.0.5" >"$STATUS_FIFO"
exit 0
STUB
chmod +x "$APP/pocketjs-kobo"

# Run with a deadline: an undrained pipe does not fail the launch, it hangs it,
# and a hanging suite reports nothing at all.
launch_before() {
    "$DEVICE_DIR/pocketjs.sh" >"$WORK/out" 2>&1 &
    pid=$!
    ticks=0
    while kill -0 "$pid" 2>/dev/null && [ "$ticks" -lt 100 ]; do
        sleep 0.05
        ticks=$((ticks + 1))
    done
    if kill -0 "$pid" 2>/dev/null; then
        kill -9 "$pid" 2>/dev/null
        echo blocked
    else
        wait "$pid"
        echo $?
    fi
}

echo running >"$NICKEL_STATE"
check "a hardware event does not block the session" "0" "$(launch_before)"
check "the UI is restored after draining" "running" "$(await_nickel)"

echo "-- restart without a reboot --"
# Booted from rcS, killing the host runs the exit trap and the Kobo UI comes
# back, so trying a different waveform used to cost a reboot.
RUN_COUNT="$WORK/runs"
RUN_ARGS="$WORK/run.args"
export RUN_COUNT RUN_ARGS
cat >"$APP/pocketjs-kobo" <<'STUB'
#!/bin/sh
runs=$(cat "$RUN_COUNT" 2>/dev/null || echo 0)
runs=$((runs + 1))
echo "$runs" >"$RUN_COUNT"
echo "$*" >"$RUN_ARGS"
[ "$runs" = 1 ] && touch "$POCKETJS_DIR/RESTART"
exit 0
STUB
chmod +x "$APP/pocketjs-kobo"

: >"$RUN_COUNT"
echo running >"$NICKEL_STATE"
check "a restart request is not a failure" "0" "$(launch)"
check "the host ran again" "2" "$(cat "$RUN_COUNT")"
check "the request is consumed" "0" \
    "$([ -e "$APP/RESTART" ] && echo 1 || echo 0)"
check "the UI comes back only at the end" "running" "$(await_nickel)"

# The point of restarting is trying something different.
: >"$RUN_COUNT"
echo "--motion-waveform A2" >"$APP/RESTART.args"
echo running >"$NICKEL_STATE"
launch >/dev/null
# The bundle arguments are always the launcher's; RESTART.args replaces only
# what the caller passed after them.
contains "the flusher is started" "flushing logs every" "$WORK/boot.log"
# It flushes the card so a freeze leaves a log, and it must stop when the
# session does: an orphan would keep syncing for as long as the device is up.
flushes_before=$(wc -l <"$SYNC_LOG")
sleep 2.5
check "the flusher does not outlive the session" "$flushes_before" \
    "$(wc -l <"$SYNC_LOG")"

check "the restart takes the new options" "ok" \
    "$(grep -q -- '--pak .* --motion-waveform A2$' "$RUN_ARGS" &&
        echo ok || cat "$RUN_ARGS")"
rm -f "$APP/RESTART.args"

echo
if [ "$FAILURES" -eq 0 ]; then
    echo "device scripts: all checks passed"
else
    echo "device scripts: $FAILURES check(s) failed"
fi
exit "$FAILURES"
