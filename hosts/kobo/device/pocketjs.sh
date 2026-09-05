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

NICKEL_SH_LIBRARY=1
export NICKEL_SH_LIBRARY
# shellcheck source=hosts/kobo/device/nickel.sh
. "$POCKETJS_STAGE/nickel.sh"

fail() {
    echo "pocketjs: $1" >&2
    exit 1
}

resolve_fbink() {
    # KOReader ships the FBInk CLI in its own install directory, which is the
    # easiest way to get a Kobo-native build onto the device.
    for candidate in \
        "${POCKETJS_FBINK:-}" \
        "$POCKETJS_DIR/bin/fbink" \
        /mnt/onboard/.adds/koreader/fbink \
        /usr/local/bin/fbink; do
        if [ -n "$candidate" ] && [ -x "$candidate" ]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

[ -x "$POCKETJS_BIN" ] || fail "host binary not found or not executable: $POCKETJS_BIN"
[ -r "$POCKETJS_JS" ] || fail "bundle not readable: $POCKETJS_JS"
[ -r "$POCKETJS_PAK" ] || fail "pak not readable: $POCKETJS_PAK"
fbink="$(resolve_fbink)" || fail "no FBInk CLI found; install one and set POCKETJS_FBINK"

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

nickel_stop || fail "nickel would not stop; refusing to fight it for /dev/fb0"

[ -f "$POCKETJS_LOG" ] && mv -f "$POCKETJS_LOG" "$POCKETJS_LOG.1"
echo "pocketjs: fbink   $fbink"
echo "pocketjs: logging $POCKETJS_LOG"

POCKETJS_GUI_PAUSED=1
export POCKETJS_GUI_PAUSED
"$POCKETJS_BIN" \
    --js "$POCKETJS_JS" \
    --pak "$POCKETJS_PAK" \
    --fbink "$fbink" \
    "$@" >>"$POCKETJS_LOG" 2>&1
