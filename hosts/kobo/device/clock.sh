#!/bin/sh
# Give the device a real time.
#
#   clock.sh sync     set the clock from NTP and persist it to the RTC
#   clock.sh status   report system time, RTC time and timezone
#
# The RTC is the source, not the network: rcS already runs `hwclock -s -u` at
# boot, so once a correct value has been written the device starts up right
# with no network at all. NTP is the correction that gets it there and keeps
# it there — this Glo came back from a factory restore reading 2012-05-01, and
# a status screen dated 2012 is not a status screen.
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
