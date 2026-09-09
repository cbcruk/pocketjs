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
    cp "$source_dir/clock.sh" "$stage/clock.sh" 2>/dev/null
    chmod +x "$stage/pocketjs.sh" "$stage/nickel.sh"
    [ -f "$stage/clock.sh" ] && chmod +x "$stage/clock.sh"
    POCKETJS_REEXEC=1
    POCKETJS_STAGE="$stage"
    export POCKETJS_REEXEC POCKETJS_STAGE POCKETJS_DIR POCKETJS_BIN POCKETJS_JS \
        POCKETJS_PAK POCKETJS_LOG POCKETJS_LOCK
    exec "$stage/pocketjs.sh" "$@"
fi

# Booted from rcS this script's own output goes to the console and is lost, so
# the one time it matters — a boot that did not work — there is nothing to
# read. Keep it next to the host's log. Interactive runs still print.
#
# `[ -t 1 ]` cannot make that distinction here: init hands rcS the console,
# which is a tty, so a boot run looked interactive and wrote no log at all.
# What actually separates the two is which terminal — a session that can read
# the output arrives over telnet on a pty; anything else has nobody watching.
POCKETJS_BOOT_LOG="${POCKETJS_BOOT_LOG:-$POCKETJS_DIR/launcher.log}"
# Keep more than one generation. A single .1 survives exactly one reboot, and
# the reboot after a crash is rarely the last one before anybody looks: two
# boots to test the network were enough to overwrite the log of the crash they
# were meant to explain.
POCKETJS_LOG_KEEP="${POCKETJS_LOG_KEEP:-4}"

rotate_log() {
    [ -f "$1" ] || return 0
    generation="$POCKETJS_LOG_KEEP"
    while [ "$generation" -gt 1 ]; do
        previous=$((generation - 1))
        [ -f "$1.$previous" ] && mv -f "$1.$previous" "$1.$generation"
        generation="$previous"
    done
    mv -f "$1" "$1.1"
}
POCKETJS_STDOUT="${POCKETJS_STDOUT:-$(readlink /proc/self/fd/1 2>/dev/null)}"
case "$POCKETJS_STDOUT" in
    /dev/pts/*) POCKETJS_WATCHED=1 ;;
    *) POCKETJS_WATCHED=0 ;;
esac
if [ "$POCKETJS_WATCHED" = 0 ]; then
    rotate_log "$POCKETJS_BOOT_LOG"
    exec >>"$POCKETJS_BOOT_LOG" 2>&1
fi
echo "pocketjs: launcher starting $(date 2>/dev/null)"

# The host reads the zone from the environment, and this firmware ships no
# zoneinfo database — musl takes the POSIX string directly. Without this a
# correct clock still shows the wrong hours, which for a status screen is the
# same as being wrong.
if [ -x "$POCKETJS_STAGE/clock.sh" ]; then
    CLOCK_SH_LIBRARY=1 . "$POCKETJS_STAGE/clock.sh"
    TZ="$(clock_timezone)"
    export TZ
    echo "pocketjs: timezone $TZ"
fi

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

# rcS makes /tmp/nickel-hardware-status and every hardware event script writes
# a line to it: udhcpc on each lease, the udev hooks on USB and SD. Nickel is
# the reader. With nickel gone the pipe has none, so each of those writers
# blocks in the kernel forever — one leaked shell per network event, for as
# long as the device stays up. Take over the read side while we own the panel,
# and hand it back before nickel does, or we would eat the lines it wants.
STATUS_FIFO="${STATUS_FIFO:-/tmp/nickel-hardware-status}"
STATUS_DRAIN_PID=""

status_drain_start() {
    [ -p "$STATUS_FIFO" ] || return 0
    # Read-write, so the pipe keeps a reader across the gaps between writers
    # and nobody ever blocks opening it. `cat` then drains what arrives.
    exec 9<>"$STATUS_FIFO" || return 0
    cat <&9 >/dev/null 2>&1 &
    STATUS_DRAIN_PID=$!
    echo "pocketjs: draining $STATUS_FIFO (pid $STATUS_DRAIN_PID)"
}

status_drain_stop() {
    [ -n "$STATUS_DRAIN_PID" ] || return 0
    kill "$STATUS_DRAIN_PID" 2>/dev/null
    exec 9>&-
    STATUS_DRAIN_PID=""
}

# /mnt/onboard is FAT32 and buffers writes, so a device that wedges hard enough
# to need the power button loses whatever its log had to say — the one session
# worth reading is the one guaranteed to be gone. Found that way: a freeze left
# a zero-byte pocketjs.log.1. Flushing on a timer bounds the loss to the
# interval instead of the whole session.
SYNC_EVERY="${POCKETJS_SYNC_SECS:-20}"
SYNC_PID=""

# There is no sleep state on this device (docs/PROGRESS.md), so how long it
# lasts on a charge is a real question with no answer yet. Sampling the gauge
# beside the flush costs one file read and gives the discharge curve for free.
BATTERY_LOG="${POCKETJS_BATTERY_LOG:-$POCKETJS_DIR/battery.log}"
BATTERY_GAUGE="${POCKETJS_BATTERY_GAUGE:-/sys/class/power_supply/mc13892_bat}"
BATTERY_EVERY="${POCKETJS_BATTERY_SECS:-300}"

log_flush_start() {
    [ "$SYNC_EVERY" -gt 0 ] 2>/dev/null || return 0
    (
        elapsed=0
        while :; do
            sleep "$SYNC_EVERY"
            elapsed=$((elapsed + SYNC_EVERY))
            if [ -r "$BATTERY_GAUGE/capacity" ] &&
                [ "$elapsed" -ge "$BATTERY_EVERY" ]; then
                elapsed=0
                echo "$(date '+%F %T') $(cat "$BATTERY_GAUGE/capacity")% \
$(cat "$BATTERY_GAUGE/status" 2>/dev/null) up$(cut -d. -f1 /proc/uptime)s" \
                    >>"$BATTERY_LOG"
            fi
            sync
        done
    ) >/dev/null 2>&1 &
    SYNC_PID=$!
    echo "pocketjs: flushing logs every ${SYNC_EVERY}s (pid $SYNC_PID)"
    [ -r "$BATTERY_GAUGE/capacity" ] &&
        echo "pocketjs: battery $(cat "$BATTERY_GAUGE/capacity")% \
-> $BATTERY_LOG every ${BATTERY_EVERY}s"
}

log_flush_stop() {
    [ -n "$SYNC_PID" ] || return 0
    kill "$SYNC_PID" 2>/dev/null
    SYNC_PID=""
    sync
}

cleanup() {
    status=$?
    trap - EXIT
    log_flush_stop
    status_drain_stop
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

    # telnetd needs a pty, and this firmware's rcS never mounts devpts —
    # nickel did it. Without this the daemon starts and then cannot serve.
    if ! grep -q " /dev/pts " /proc/mounts 2>/dev/null; then
        mkdir -p /dev/pts
        mount -t devpts devpts /dev/pts 2>/dev/null ||
            echo "pocketjs: could not mount devpts" >&2
    fi
    # The multiplexer is the other half of a pty pair. udev makes it when it
    # runs; /dev is a tmpfs built fresh each boot, so do not assume it did.
    if [ ! -e /dev/ptmx ]; then
        mknod /dev/ptmx c 5 2 && chmod 666 /dev/ptmx
    fi

    # The RTC survives a reboot and the network may not, so this is a
    # correction rather than the source. Backgrounded: a boot should not wait
    # on a name server, and the host is told to re-read the clock when the
    # answer arrives — publish_boot_clock runs again on SIGHUP.
    if [ -x "$POCKETJS_STAGE/clock.sh" ]; then
        (
            sh "$POCKETJS_STAGE/clock.sh" sync &&
                killall -HUP pocketjs-kobo 2>/dev/null
        ) &
    fi

    # Not `pidof telnetd`: started as a busybox applet the process is named
    # busybox, so that asks a question it can never answer yes to. What we
    # actually want to know is whether anything holds the port.
    if awk '$2 ~ /:0017$/ && $4 == "0A" { found = 1 } END { exit !found }' \
        /proc/net/tcp 2>/dev/null; then
        echo "pocketjs: telnet port is already served"
    # The applet is inside busybox, but 2.1.5 ships no symlink for it, so
    # calling it by name finds nothing. root has an empty password field and
    # /bin/login exists, which is the same way in the firmware's own debug
    # services offer.
    elif command -v telnetd >/dev/null 2>&1; then
        telnetd && echo "pocketjs: telnetd listening"
    elif [ -x /bin/busybox ]; then
        /bin/busybox telnetd && echo "pocketjs: telnetd listening (busybox applet)"
    else
        echo "pocketjs: no telnetd to start" >&2
    fi
fi

nickel_stop || fail "nickel would not stop; refusing to fight it for /dev/fb0"
status_drain_start
log_flush_start

rotate_log "$POCKETJS_LOG"
echo "pocketjs: logging $POCKETJS_LOG"

POCKETJS_GUI_PAUSED=1
export POCKETJS_GUI_PAUSED

# Booted from rcS this launcher owns the screen for the life of the device, so
# every host change used to cost a reboot: killing the host runs the exit trap,
# and the exit trap correctly brings the Kobo UI back. Leave a RESTART file and
# the host comes back instead — which is what trying a waveform looks like.
#
#   kobo-push.py <ip> <dir> pocketjs-kobo
#   kobo-sh.py <ip> "touch <dir>/RESTART; killall pocketjs-kobo"
# Falling back to the Kobo UI is right for a session that never started and
# wrong for one that ran for hours and fell over: nickel takes the panel and
# the radio with it, so a crash at 3am reads to the owner as a dead device,
# and every one of the four "the device froze" reports was exactly this.
#
# The two cases are told apart by how long the session lived. A run that
# reached HEALTHY_SECONDS was working, so it earns the counter back and its
# death is worth retrying; RETRY_LIMIT consecutive deaths short of that is a
# device that cannot run this at all, and the Kobo UI is the better answer.
#
# One threshold, not two. An earlier version reset the counter at the same
# short mark it used to judge a failed start, which made the limit unreachable
# for anything that crashed just after it — a host dying every 61 seconds
# would have retried forever.
HEALTHY_SECONDS="${POCKETJS_HEALTHY_SECONDS:-600}"
RETRY_LIMIT="${POCKETJS_RETRY_LIMIT:-5}"
short_runs=0

while :; do
    began="$(date +%s 2>/dev/null || echo 0)"
    "$POCKETJS_BIN" \
        --js "$POCKETJS_JS" \
        --pak "$POCKETJS_PAK" \
        "$@" >>"$POCKETJS_LOG" 2>&1
    status=$?
    ended="$(date +%s 2>/dev/null || echo 0)"
    lived=$((ended - began))

    if [ ! -e "$POCKETJS_DIR/RESTART" ]; then
        # Zero means the host decided to stop — a signal, or the long press
        # that hands the device back. Only a failure is worth arguing with.
        [ "$status" -eq 0 ] && break

        if [ "$lived" -ge "$HEALTHY_SECONDS" ]; then
            short_runs=1
        else
            short_runs=$((short_runs + 1))
        fi
        if [ "$short_runs" -ge "$RETRY_LIMIT" ]; then
            echo "pocketjs: $short_runs runs in a row died inside \
${HEALTHY_SECONDS}s; handing the device back" >&2
            break
        fi
        echo "pocketjs: host exited $status after ${lived}s; restarting \
(short runs: $short_runs/$RETRY_LIMIT)" >&2
        echo "--- crash restart $(date 2>/dev/null) ---" >>"$POCKETJS_LOG"
        sync
        continue
    fi
    # Consume it first: a restart that then fails must not loop forever.
    rm -f "$POCKETJS_DIR/RESTART"
    read_args=$(cat "$POCKETJS_DIR/RESTART.args" 2>/dev/null) || read_args=""
    if [ -n "$read_args" ]; then
        echo "pocketjs: restarting with $read_args"
        # Deliberate word splitting: the file holds a command line.
        # shellcheck disable=SC2086
        set -- $read_args
    else
        echo "pocketjs: restarting"
    fi
    # Append rather than rotate. Rotating here costs one generation per
    # restart, and the generation it costs is the one holding the session that
    # just wedged — which is how the log of the first freeze was lost, to the
    # very next restart made to investigate it.
    echo "--- restart $(date 2>/dev/null) ---" >>"$POCKETJS_LOG"
    # Which options this run is about to use, on the card before it runs them.
    # If it wedges, that line is the whole diagnosis.
    sync
done
exit "$status"
