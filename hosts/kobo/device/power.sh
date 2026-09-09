#!/bin/sh
# Suspend the Kobo to RAM, and put it back together on the way out.
#
#   power.sh doze | wake | suspend | status
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
# `doze` is what this device actually gets. Suspend-to-RAM hangs its kernel in
# the driver-suspend stage — proved with the kernel's own pm_test, with the
# host stopped and Wi-Fi down, so it is not something userspace is doing wrong
# (hosts/kobo/docs/PROGRESS.md). Dozing keeps the machine up and drops the
# radio; the caller stops ticking the guest.
#
# It does NOT touch the CPU frequency, and an earlier version that did was
# measuring the wrong thing. cpufreq reports a `userspace` governor sitting at
# 800MHz, which reads like a chip pinned at its maximum — but DVFS is enabled
# underneath and scales the part regardless. Sampled ten times a second:
# 176MHz average with the runtime ticking, 160MHz (the floor) with it stopped.
# There was nothing there to save.
POWER_CPUFREQ="$POWER_ROOT/sys/devices/system/cpu/cpu0/cpufreq"
POWER_DOZE_STATE="${POWER_DOZE_STATE:-/tmp/pocketjs-doze}"

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
    # The governor line is informational and misleading on its own: DVFS moves
    # the part underneath it, so cur_freq is the only number worth reading.
    echo "cpu            $(cpufreq_read scaling_cur_freq) kHz now \
(range $(cpufreq_read scaling_min_freq)-$(cpufreq_read scaling_max_freq), \
governor $(cpufreq_read scaling_governor) over DVFS)"
    echo "dozing         $([ -f "$POWER_DOZE_STATE" ] && echo yes || echo no)"
    grep -q mem "$POWER_STATE" 2>/dev/null
}

cpufreq_read() {
    cat "$POWER_CPUFREQ/$1" 2>/dev/null
}

power_doze() {
    if [ -f "$POWER_DOZE_STATE" ]; then
        echo "power: already dozing"
        return 0
    fi
    : >"$POWER_DOZE_STATE"

    if [ "$POWER_RESTORE_WIFI" = "1" ] && [ -x "$(power_dir)/wifi.sh" ]; then
        "$(power_dir)/wifi.sh" down
    fi
    echo "power: dozing — radio down, cpu left to DVFS ($(cpufreq_read scaling_cur_freq) kHz)"
}

power_wake() {
    [ -f "$POWER_DOZE_STATE" ] || {
        echo "power: not dozing"
        return 0
    }
    rm -f "$POWER_DOZE_STATE"

    if [ "$POWER_RESTORE_WIFI" = "1" ] && [ -x "$(power_dir)/wifi.sh" ]; then
        "$(power_dir)/wifi.sh" up
    fi
    echo "power: awake"
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
        doze) power_doze ;;
        wake) power_wake ;;
        status) power_status ;;
        *)
            echo "usage: power.sh doze | wake | suspend | status" >&2
            exit 2
            ;;
    esac
fi
