#!/bin/sh
# Suspend the Kobo to RAM, and put it back together on the way out.
#
#   power.sh suspend | status
#
# Nickel normally owns this. A session that replaced nickel has to do it
# itself, or the device runs the battery down staying awake — a reader that
# cannot sleep is not a reader.
#
# The sequence is KOReader's (frontend/device/kobo/device.lua), which is the
# tested one for this platform:
#
#   state-extended=1 -> wait 2s -> sync -> state=mem   [blocks until woken]
#   -> state-extended=0 -> settle -> IR grid knob -> Wi-Fi back
#
# `state-extended` is the NTX addition that flags the subsystems; suspending
# without it is how the panel and the PMIC come back wrong.
#
# THE ONLY WAY BACK IS THE POWER BUTTON. This rtc has no `wakealarm`, so there
# is no timer to fall back on: someone has to be holding the device.

set -u

POWER_ROOT="${POWER_ROOT:-}"
POWER_STATE="$POWER_ROOT/sys/power/state"
POWER_STATE_EXTENDED="$POWER_ROOT/sys/power/state-extended"
# Nickel sleeps here too. Nobody has written down exactly why.
POWER_SETTLE="${POWER_SETTLE:-2}"
# The Neonode grid on some models needs a nudge or it chatters after resume.
POWER_IR_KNOBS="/sys/devices/virtual/input/input1/neocmd
/sys/devices/platform/imx-i2c.1/i2c-1/1-0050/neocmd"
# Wi-Fi does not survive a suspend on this chip, so it is taken down first and
# brought back after. Set to 0 to leave the interface alone.
POWER_RESTORE_WIFI="${POWER_RESTORE_WIFI:-1}"

power_dir() {
    dirname "$(readlink -f "$0" 2>/dev/null || echo "$0")"
}

power_status() {
    echo "state          $(cat "$POWER_STATE" 2>/dev/null || echo '(unreadable)')"
    echo "state-extended $([ -w "$POWER_STATE_EXTENDED" ] && echo writable || echo missing)"
    echo "wakealarm      $([ -e /sys/class/rtc/rtc0/wakealarm ] && echo present ||
        echo 'absent — the power button is the only way back')"
    for knob in $POWER_IR_KNOBS; do
        [ -f "$knob" ] && echo "ir grid        $knob"
    done
    grep -q mem "$POWER_STATE" 2>/dev/null
}

power_suspend() {
    grep -q mem "$POWER_STATE" 2>/dev/null || {
        echo "power: this kernel does not offer suspend to RAM" >&2
        return 1
    }

    if [ "$POWER_RESTORE_WIFI" = "1" ] && [ -x "$(power_dir)/wifi.sh" ]; then
        "$(power_dir)/wifi.sh" down
    fi

    if ! echo 1 >"$POWER_STATE_EXTENDED" 2>/dev/null; then
        echo "power: the kernel refused to flag subsystems for suspend" >&2
        return 1
    fi
    sleep "$POWER_SETTLE"
    sync

    echo "power: suspending — the power button is the way back"
    if echo mem >"$POWER_STATE" 2>/dev/null; then
        echo "power: woke up"
    else
        echo "power: the kernel refused to suspend" >&2
        echo 0 >"$POWER_STATE_EXTENDED" 2>/dev/null
        return 1
    fi

    echo 0 >"$POWER_STATE_EXTENDED" 2>/dev/null ||
        echo "power: the kernel refused to un-flag subsystems" >&2
    usleep 100000
    for knob in $POWER_IR_KNOBS; do
        [ -f "$knob" ] && echo a >"$knob" 2>/dev/null
    done

    if [ "$POWER_RESTORE_WIFI" = "1" ] && [ -x "$(power_dir)/wifi.sh" ]; then
        "$(power_dir)/wifi.sh" up
    fi
    echo "power: resumed"
}

if [ "${POWER_SH_LIBRARY:-0}" != "1" ]; then
    case "${1:-}" in
        suspend) power_suspend ;;
        status) power_status ;;
        *)
            echo "usage: power.sh suspend | status" >&2
            exit 2
            ;;
    esac
fi
