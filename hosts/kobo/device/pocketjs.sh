#!/bin/sh
# PocketJS Kobo launcher.
#
#   pocketjs.sh [host options...]
#
# Pauses nickel, runs the host, and restores nickel on every exit path,
# including a crash or a kill. Extra arguments are passed to the host, so
# tuning a session is `pocketjs.sh --motion-waveform A2 --ghost-budget 40`.
#
# Send SIGHUP to reload the bundle without restarting the session:
#   killall -HUP pocketjs-kobo

set -u

POCKETJS_DIR="${POCKETJS_DIR:-/mnt/onboard/.apps/pocketjs}"
POCKETJS_BIN="${POCKETJS_BIN:-$POCKETJS_DIR/pocketjs-kobo}"
POCKETJS_JS="${POCKETJS_JS:-$POCKETJS_DIR/app.js}"
POCKETJS_PAK="${POCKETJS_PAK:-$POCKETJS_DIR/app.pak}"
POCKETJS_LOG="${POCKETJS_LOG:-$POCKETJS_DIR/pocketjs.log}"
POCKETJS_LOCK="${POCKETJS_LOCK:-/tmp/pocketjs.lock}"

# Run the long-lived shell from tmpfs. /mnt/onboard is FAT32 and disappears the
# moment the device is plugged into a computer; if this script's own text went
# with it, nickel would never be restored.
if [ "${POCKETJS_REEXEC:-0}" != "1" ]; then
    stage="/tmp/pocketjs-launcher.$$"
    source_dir="$(dirname "$0")"
    rm -rf "$stage" || exit 1
    mkdir -p "$stage" || exit 1
    cp "$0" "$stage/pocketjs.sh" || exit 1
    cp "$source_dir/nickel.sh" "$stage/nickel.sh" || exit 1
    chmod +x "$stage/pocketjs.sh" "$stage/nickel.sh"
    POCKETJS_REEXEC=1
    POCKETJS_STAGE="$stage"
    export POCKETJS_REEXEC POCKETJS_STAGE POCKETJS_DIR POCKETJS_BIN POCKETJS_JS \
        POCKETJS_PAK POCKETJS_LOG POCKETJS_LOCK
    exec "$stage/pocketjs.sh" "$@"
fi

# Booted from rcS this script's own output goes to the console and is lost, so
# the one time it matters — a boot that did not work — there is nothing to
# read. Keep it next to the host's log. Interactive runs still print.
POCKETJS_BOOT_LOG="${POCKETJS_BOOT_LOG:-$POCKETJS_DIR/launcher.log}"
if [ ! -t 1 ]; then
    [ -f "$POCKETJS_BOOT_LOG" ] && mv -f "$POCKETJS_BOOT_LOG" "$POCKETJS_BOOT_LOG.1"
    exec >>"$POCKETJS_BOOT_LOG" 2>&1
fi
echo "pocketjs: launcher starting $(date 2>/dev/null)"

NICKEL_SH_LIBRARY=1
export NICKEL_SH_LIBRARY
# shellcheck source=hosts/kobo/device/nickel.sh
. "$POCKETJS_STAGE/nickel.sh"

fail() {
    echo "pocketjs: $1" >&2
    exit 1
}

[ -x "$POCKETJS_BIN" ] || fail "host binary not found or not executable: $POCKETJS_BIN"
[ -r "$POCKETJS_JS" ] || fail "bundle not readable: $POCKETJS_JS"
[ -r "$POCKETJS_PAK" ] || fail "pak not readable: $POCKETJS_PAK"

if ! mkdir "$POCKETJS_LOCK" 2>/dev/null; then
    if [ -r "$POCKETJS_LOCK/pid" ] && kill -0 "$(cat "$POCKETJS_LOCK/pid")" 2>/dev/null; then
        fail "already running as pid $(cat "$POCKETJS_LOCK/pid")"
    fi
    echo "pocketjs: clearing stale lock $POCKETJS_LOCK" >&2
    rm -rf "$POCKETJS_LOCK"
    mkdir "$POCKETJS_LOCK" || fail "cannot take lock $POCKETJS_LOCK"
fi
echo $$ >"$POCKETJS_LOCK/pid"

cleanup() {
    status=$?
    trap - EXIT
    nickel_start
    rm -rf "$POCKETJS_LOCK"
    [ -n "${POCKETJS_STAGE:-}" ] && rm -rf "$POCKETJS_STAGE"
    exit "$status"
}
# Signals exit through the same path so the UI comes back either way.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

# Measured on a Kobo Glo with --probe-touch, not assumed: the zForce reports
# panel pixels while declaring 0..1200 by 0..1600, and its axes are swapped
# with X mirrored. Every value stays overridable from the environment.
POCKETJS_TOUCH_SWAP_XY="${POCKETJS_TOUCH_SWAP_XY:-1}"
POCKETJS_TOUCH_FLIP_X="${POCKETJS_TOUCH_FLIP_X:-1}"
POCKETJS_TOUCH_FLIP_Y="${POCKETJS_TOUCH_FLIP_Y:-0}"
POCKETJS_TOUCH_X_MAX="${POCKETJS_TOUCH_X_MAX:-1023}"
POCKETJS_TOUCH_Y_MAX="${POCKETJS_TOUCH_Y_MAX:-757}"
export POCKETJS_TOUCH_SWAP_XY POCKETJS_TOUCH_FLIP_X POCKETJS_TOUCH_FLIP_Y \
    POCKETJS_TOUCH_X_MAX POCKETJS_TOUCH_Y_MAX

# Optional remote access, opted into by a file rather than by default. Booted
# from rcS there is no UI to turn Wi-Fi on with, and no nickel to run telnetd,
# so without this the device is unreachable and every change costs a trip
# through the card reader. It is off unless asked for because what it opens is
# a root shell with no password on the local network — the firmware's own
# debug behaviour, but not something a device should do silently.
if [ -e "$POCKETJS_DIR/REMOTE" ]; then
    echo "pocketjs: REMOTE present; bringing the network up"
    sh "$POCKETJS_DIR/wifi.sh" up || echo "pocketjs: could not bring Wi-Fi up" >&2
    if ! pidof telnetd >/dev/null 2>&1; then
        telnetd && echo "pocketjs: telnetd listening"
    fi
fi

nickel_stop || fail "nickel would not stop; refusing to fight it for /dev/fb0"

[ -f "$POCKETJS_LOG" ] && mv -f "$POCKETJS_LOG" "$POCKETJS_LOG.1"
echo "pocketjs: logging $POCKETJS_LOG"

POCKETJS_GUI_PAUSED=1
export POCKETJS_GUI_PAUSED
"$POCKETJS_BIN" \
    --js "$POCKETJS_JS" \
    --pak "$POCKETJS_PAK" \
    "$@" >>"$POCKETJS_LOG" 2>&1
