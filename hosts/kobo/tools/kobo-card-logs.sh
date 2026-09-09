#!/bin/sh
# Read a takeover boot's logs straight off the card, for when the network is
# the thing that failed.
#
#   tools/kobo-card-logs.sh [volume]
#
# The device writes launcher.log and pocketjs.log to the user partition, which
# is FAT32 and mounts on any machine. That matters because the usual way in —
# telnet — needs the Wi-Fi that these logs exist to explain.

set -u

VOLUME="${1:-/Volumes/KOBOeReader}"
APP="$VOLUME/.apps/pocketjs"

if [ ! -d "$APP" ]; then
    echo "no PocketJS install at $APP" >&2
    echo "insert the card and check: ls /Volumes" >&2
    exit 1
fi

show() {
    [ -f "$2" ] || { echo "== $1: absent"; echo; return; }
    echo "== $1 ($(wc -c <"$2" | tr -d ' ') bytes, $(date -r "$2" '+%F %T'))"
    tail -n "${3:-40}" "$2"
    echo
}

# Newest first: the session that just died is the one being asked about.
show "launcher.log — why the boot went the way it did" "$APP/launcher.log" 40
show "pocketjs.log — the running session" "$APP/pocketjs.log" 30
show "pocketjs.log.1 — the session before it" "$APP/pocketjs.log.1" 30
show "launcher.log.1" "$APP/launcher.log.1" 20

echo "== how the last session ended"
for log in "$APP/pocketjs.log" "$APP/pocketjs.log.1"; do
    [ -f "$log" ] || continue
    # A clean exit says so. Anything else left its reason on the last line.
    printf '%s: ' "$(basename "$log")"
    tail -n 3 "$log" | grep -qE "exiting cleanly" &&
        echo "clean exit" ||
        tail -n 2 "$log" | tr '\n' ' ' | sed 's/  */ /g'
    echo
done
