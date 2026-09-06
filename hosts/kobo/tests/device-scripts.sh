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
export POCKETJS_DIR POCKETJS_LOCK

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
contains "the refusal names the holder" "already running as pid" "$WORK/out"
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
contains "the failure explains itself" "pak not readable" "$WORK/out"
check "the UI is never stopped when the launch fails early" "running" \
    "$(cat "$NICKEL_STATE")"
mv "$WORK/pak.hidden" "$APP/app.pak"

echo
if [ "$FAILURES" -eq 0 ]; then
    echo "device scripts: all checks passed"
else
    echo "device scripts: $FAILURES check(s) failed"
fi
exit "$FAILURES"
