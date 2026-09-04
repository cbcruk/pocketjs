#!/bin/sh
# Pause and resume the Kobo UI around a PocketJS session.
#
#   nickel.sh stop | start | status
#
# The restart sequence follows KOReader's platform/kobo/nickel.sh, the
# reference implementation for bringing nickel back without a reboot.
#
# Wi-Fi is deliberately left alone. KOReader tears the interface down before
# restarting nickel, but the PocketJS development loop runs over telnet or SSH,
# so doing that here would sever the session driving it. Nickel can be odd
# about an interface it did not bring up itself; reboot if it misbehaves.

set -u

# Prefix for every firmware path below. Empty on the device; the script test
# under hosts/kobo/tests points it at a staged root so the restore path can be
# exercised without a Kobo.
NICKEL_ROOT="${NICKEL_ROOT:-}"

NICKEL_PROCESSES="nickel hindenburg sickel fickel adobehost foxitpdf iink dhcpcd-dbus dhcpcd fmon"
# In 250ms ticks.
NICKEL_STOP_TIMEOUT="${NICKEL_STOP_TIMEOUT:-20}"

nickel_running() {
    pidof nickel >/dev/null 2>&1
}

nickel_stop() {
    if ! nickel_running; then
        echo "nickel: already stopped"
        return 0
    fi
    # shellcheck disable=SC2086
    killall -q -TERM $NICKEL_PROCESSES 2>/dev/null
    ticks=0
    while nickel_running; do
        if [ "$ticks" -ge "$NICKEL_STOP_TIMEOUT" ]; then
            echo "nickel: still alive after $((NICKEL_STOP_TIMEOUT / 4))s" >&2
            return 1
        fi
        usleep 250000
        ticks=$((ticks + 1))
    done
    echo "nickel: stopped"
}

nickel_start() {
    if nickel_running; then
        echo "nickel: already running"
        return 0
    fi
    if [ -e "$NICKEL_ROOT/etc/init.d/z-nickel-hardware-status" ]; then
        echo "nickel: this firmware starts nickel from /etc/init.d, which the Glo" >&2
        echo "nickel: restart path below does not cover. Reboot to recover the UI." >&2
        return 1
    fi

    LD_LIBRARY_PATH="$NICKEL_ROOT/usr/local/Kobo"
    export LD_LIBRARY_PATH
    cd "$NICKEL_ROOT/" || return 1
    unset OLDPWD

    # Brings fmon back. Nickel kills the animator itself on startup, so this
    # background job needs no reaping. The delay is what KOReader applies on
    # the i.MX5 platforms, which is what a Glo is.
    if [ -x "$NICKEL_ROOT/etc/init.d/on-animator.sh" ]; then
        (
            usleep 400000
            "$NICKEL_ROOT/etc/init.d/on-animator.sh"
        ) &
    fi

    # udev writes to this FIFO and nickel reads it. rcS creates it at boot, so
    # a restart has to recreate it too.
    rm -f "$NICKEL_ROOT/tmp/nickel-hardware-status"
    mkfifo "$NICKEL_ROOT/tmp/nickel-hardware-status"
    sync

    if [ -x "$NICKEL_ROOT/usr/local/Kobo/hindenburg" ]; then
        "$NICKEL_ROOT/usr/local/Kobo/hindenburg" &
    fi
    LIBC_FATAL_STDERR_=1 "$NICKEL_ROOT/usr/local/Kobo/nickel" -platform kobo -skipFontLoad &
    echo "nickel: restarted"
}

nickel_status() {
    if nickel_running; then
        echo "nickel: running (pid $(pidof nickel))"
    else
        echo "nickel: stopped"
    fi
}

# pocketjs.sh sets this before sourcing, because a sourced script inherits the
# caller's positional parameters and would otherwise dispatch on them.
if [ "${NICKEL_SH_LIBRARY:-0}" != "1" ]; then
    case "${1:-}" in
        stop) nickel_stop ;;
        start) nickel_start ;;
        status) nickel_status ;;
        *)
            echo "usage: nickel.sh stop | start | status" >&2
            exit 2
            ;;
    esac
fi
