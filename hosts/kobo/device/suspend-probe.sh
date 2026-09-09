#!/bin/sh
# Find out where suspend breaks, leaving evidence that survives the break.
#
#   suspend-probe.sh [freezer|devices|platform|processors|core|none] [--keep-wifi]
#
# `echo mem > /sys/power/state` on this device has hung twice, and a hung
# device cannot tell you why. Two things make it answerable:
#
# The kernel's pm_test walks the same path and then resumes itself after five
# seconds without ever cutting power, so every level except `none` comes back
# on its own. Start shallow and climb: the level that stops coming back is the
# one that owns the bug.
#
# And the log goes to the card, synced after every line. A probe whose failure
# mode is "hold the power button" must not write its notes to a tmpfs.
#
# Wi-Fi goes down first and the host is stopped, because that is what the real
# sequence does and a test that skips them is testing something else. Stopping
# the host means SIGSTOP, not kill: killing it runs the launcher's exit trap,
# which brings the Kobo UI back and ends the session under test.

set -u

LEVEL="${1:-freezer}"
KEEP_WIFI=0
[ "${2:-}" = "--keep-wifi" ] && KEEP_WIFI=1

POCKETJS_DIR="${POCKETJS_DIR:-/mnt/onboard/.apps/pocketjs}"
LOG="${SUSPEND_PROBE_LOG:-$POCKETJS_DIR/suspend-probe.log}"
SETTLE="${SUSPEND_PROBE_SETTLE:-2}"

note() {
    echo "$(date '+%H:%M:%S') $*" >>"$LOG"
    sync
}

[ -f "$LOG" ] && mv -f "$LOG" "$LOG.1"
: >"$LOG"
note "probe start: level=$LEVEL keep-wifi=$KEEP_WIFI"
note "kernel offers: $(cat /sys/power/state 2>&1) / pm_test: $(cat /sys/power/pm_test 2>&1)"

HOST_PID="$(pidof pocketjs-kobo 2>/dev/null || true)"
if [ -n "$HOST_PID" ]; then
    kill -STOP "$HOST_PID" 2>/dev/null &&
        note "host $HOST_PID stopped (SIGSTOP; it still holds /dev/fb0, it just stops drawing)"
fi

if [ "$KEEP_WIFI" = 0 ] && [ -x "$POCKETJS_DIR/wifi.sh" ]; then
    note "taking wi-fi down — this ends any telnet session, which is why this runs detached"
    sh "$POCKETJS_DIR/wifi.sh" down >>"$LOG" 2>&1
    sync
fi

dmesg -c >/dev/null 2>&1
if [ "$LEVEL" = none ]; then
    echo none >/sys/power/pm_test 2>/dev/null
    note "pm_test off — this is the real thing, and only the power button ends it"
else
    if ! echo "$LEVEL" >/sys/power/pm_test 2>/dev/null; then
        note "kernel does not know pm_test level $LEVEL"
        exit 2
    fi
    note "pm_test=$LEVEL — the kernel resumes itself after five seconds"
fi

if echo 1 >/sys/power/state-extended 2>/dev/null; then
    note "state-extended=1 (the NTX flag the tested sequence sets)"
else
    note "state-extended refused — continuing without it"
fi
sleep "$SETTLE"
sync

note "entering mem"
echo mem >/sys/power/state 2>>"$LOG"
note "returned from mem (exit $?)"

echo 0 >/sys/power/state-extended 2>/dev/null
echo none >/sys/power/pm_test 2>/dev/null
note "--- dmesg ---"
dmesg >>"$LOG" 2>&1
note "--- end dmesg ---"

if [ "$KEEP_WIFI" = 0 ] && [ -x "$POCKETJS_DIR/wifi.sh" ]; then
    sh "$POCKETJS_DIR/wifi.sh" up >>"$LOG" 2>&1
    note "wi-fi restored: $(sh "$POCKETJS_DIR/wifi.sh" status 2>/dev/null | head -1)"
fi

if [ -n "$HOST_PID" ]; then
    kill -CONT "$HOST_PID" 2>/dev/null && note "host $HOST_PID resumed"
fi
note "probe done"
