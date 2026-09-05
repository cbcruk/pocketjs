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

# Deliberately WITHOUT the DHCP clients. KOReader kills them here because it
# brings Wi-Fi up itself; we keep the network the session is running over.
# dhcpcd deconfigures its interface and releases the lease on SIGTERM, so
# killing it is how a session silently drops off the network it arrived on.
NICKEL_PROCESSES="nickel hindenburg sickel fickel adobehost foxitpdf iink fmon"
# Handed back to nickel at restart, since nickel expects to own them.
NICKEL_DHCP_PROCESSES="dhcpcd-dbus dhcpcd"
# In 250ms ticks.
NICKEL_STOP_TIMEOUT="${NICKEL_STOP_TIMEOUT:-20}"
# Where the environment nickel was running with gets parked while it is down.
NICKEL_ENV_CACHE="${NICKEL_ENV_CACHE:-/tmp/pocketjs-nickel-env}"

# The variables /etc/init.d/rcS exports before starting nickel. Restarting it
# without them leaves a subtly broken UI: WIFI_MODULE_PATH in particular
# collapses to /drivers//wifi/.ko, so nothing can bring the radio back.
NICKEL_ENV_KEYS="PLATFORM PRODUCT INTERFACE WIFI_MODULE WIFI_MODULE_PATH \
NICKEL_HOME LD_LIBRARY_PATH LANG DBUS_SESSION_BUS_ADDRESS"

nickel_running() {
    pidof nickel >/dev/null 2>&1
}

# Snapshot the live process's environment before killing it. Reading it back
# off /proc is better than deriving it: it preserves whatever the firmware set,
# including the dbus session address, which cannot be reconstructed.
nickel_capture_env() {
    pid="$(pidof nickel 2>/dev/null | cut -d' ' -f1)"
    [ -n "$pid" ] && [ -r "/proc/$pid/environ" ] || return 0
    captured="$(tr '\0' '\n' <"/proc/$pid/environ" 2>/dev/null)" || return 0
    : >"$NICKEL_ENV_CACHE"
    for key in $NICKEL_ENV_KEYS; do
        line="$(echo "$captured" | grep "^$key=" | head -n 1)"
        [ -n "$line" ] && echo "$line" >>"$NICKEL_ENV_CACHE"
    done
}

# Restore the snapshot, then fill any gap the way rcS derives it. A shell that
# arrived over telnet inherits none of this, so without the fallbacks a restart
# from a debug session hands nickel an empty environment.
nickel_restore_env() {
    if [ -r "$NICKEL_ENV_CACHE" ]; then
        while IFS= read -r line; do
            [ -n "$line" ] && export "$line"
        done <"$NICKEL_ENV_CACHE"
    fi

    if [ -z "${PLATFORM:-}" ]; then
        cpu="$(ntx_hwconfig -s -p /dev/mmcblk0 CPU 2>/dev/null)"
        PLATFORM="freescale"
        [ -n "$cpu" ] && PLATFORM="$cpu-ntx"
    fi
    [ -n "${PRODUCT:-}" ] || PRODUCT="$(kobo_config.sh 2>/dev/null)"
    if [ "$PLATFORM" = "freescale" ]; then
        [ -n "${INTERFACE:-}" ] || INTERFACE="wlan0"
        [ -n "${WIFI_MODULE:-}" ] || WIFI_MODULE="ar6000"
    else
        [ -n "${INTERFACE:-}" ] || INTERFACE="eth0"
        [ -n "${WIFI_MODULE:-}" ] || WIFI_MODULE="dhd"
    fi
    [ -n "${WIFI_MODULE_PATH:-}" ] ||
        WIFI_MODULE_PATH="$NICKEL_ROOT/drivers/$PLATFORM/wifi/$WIFI_MODULE.ko"
    [ -n "${NICKEL_HOME:-}" ] || NICKEL_HOME="$NICKEL_ROOT/mnt/onboard/.kobo"
    [ -n "${LANG:-}" ] || LANG="en_US.UTF-8"
    LD_LIBRARY_PATH="$NICKEL_ROOT/usr/local/Kobo"
    export PLATFORM PRODUCT INTERFACE WIFI_MODULE WIFI_MODULE_PATH NICKEL_HOME \
        LANG LD_LIBRARY_PATH

    if [ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]; then
        echo "nickel: no dbus session address to restore; the UI may be degraded" >&2
    fi
}

nickel_stop() {
    if ! nickel_running; then
        echo "nickel: already stopped"
        return 0
    fi
    nickel_capture_env
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

    nickel_restore_env
    # nickel brings its own DHCP client up, so hand the interface back before
    # starting it. Stopping did not touch these on purpose (see above).
    # shellcheck disable=SC2086
    killall -q -TERM $NICKEL_DHCP_PROCESSES 2>/dev/null
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
