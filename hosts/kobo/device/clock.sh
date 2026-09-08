#!/bin/sh
# Give the device a real time.
#
#   clock.sh sync     set the clock from NTP and persist it to the RTC
#   clock.sh status   report system time, RTC time and timezone
#
# NTP is the correction; whether the RTC is a source is device-dependent and
# on this Glo it appears not to be. rcS runs `hwclock -s -u` at boot, so a
# device whose RTC holds time comes up right with no network — but this one
# came back from a power cycle reading 2012-05-01 again, having been written
# a correct value hours earlier. A 2012 device with a spent backup cell is the
# obvious explanation, and rcS backgrounding that hwclock call is the other
# one; `clock_sync` logs what the RTC held so a cold boot decides it.
#
# Either way the screen must not show 2012, which is what this is for.
#
# The timezone is a POSIX TZ string in $POCKETJS_DIR/timezone, because this
# firmware ships no zoneinfo database and musl reads the string directly.
# Seoul is `KST-9` — POSIX counts east as negative, so that is UTC+9.

set -u

POCKETJS_DIR="${POCKETJS_DIR:-/mnt/onboard/.apps/pocketjs}"
CLOCK_NTP_PEER="${CLOCK_NTP_PEER:-pool.ntp.org}"
CLOCK_TZ_FILE="${CLOCK_TZ_FILE:-$POCKETJS_DIR/timezone}"

clock_timezone() {
    [ -r "$CLOCK_TZ_FILE" ] || { echo UTC; return; }
    # First non-blank, non-comment line. A whole file is not a TZ string.
    sed -e 's/#.*//' -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//' "$CLOCK_TZ_FILE" |
        grep -v '^[[:space:]]*$' | head -1 | grep . || echo UTC
}

clock_sync() {
    # What the RTC was holding before anything corrected it. A cold boot that
    # comes up in 2012 has either a backup cell that no longer holds charge or
    # an rcS whose `hwclock -s -u` — which it backgrounds — had not run yet,
    # and those want opposite fixes. Recording it costs nothing and lets the
    # next power cycle answer instead of the next argument.
    echo "clock: rtc held $(hwclock -r -u 2>&1 | head -1)"
    if ! ntpd -q -n -p "$CLOCK_NTP_PEER" 2>&1; then
        echo "clock: no answer from $CLOCK_NTP_PEER; keeping the RTC" >&2
        return 1
    fi
    # Persist in UTC. The RTC has no notion of a zone, and rcS reads it back
    # with -u, so writing local time would drift the device by the offset on
    # every reboot.
    hwclock -w -u || {
        echo "clock: could not write the RTC; this boot is right, the next is not" >&2
        return 1
    }
    echo "clock: $(TZ="$(clock_timezone)" date)"
}

clock_status() {
    echo "timezone   $(clock_timezone) (from $CLOCK_TZ_FILE)"
    echo "local      $(TZ="$(clock_timezone)" date)"
    echo "utc        $(date -u)"
    echo "rtc        $(hwclock -r -u 2>&1)"
}

if [ "${CLOCK_SH_LIBRARY:-0}" != "1" ]; then
    case "${1:-}" in
        sync) clock_sync ;;
        status) clock_status ;;
        *)
            echo "usage: clock.sh sync | status" >&2
            exit 2
            ;;
    esac
fi
