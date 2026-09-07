#!/bin/sh
# Read-only device report for PocketJS Kobo bring-up.
#
#   diagnose.sh [> report.txt]
#
# Nothing here writes /dev/fb0 or stops nickel, so it is safe to run from a
# telnet or SSH session on a device that is otherwise in normal use. It
# collects what the two open device questions need: the exact visible
# framebuffer, and the touchscreen's evdev contract.

set -u

POCKETJS_DIR="${POCKETJS_DIR:-/mnt/onboard/.apps/pocketjs}"
POCKETJS_BIN="${POCKETJS_BIN:-$POCKETJS_DIR/pocketjs-kobo}"

section() {
    echo
    echo "=== $1 ==="
}

section "device"
[ -r /mnt/onboard/.kobo/version ] && cat /mnt/onboard/.kobo/version
uname -a
[ -x /bin/kobo_config.sh ] && echo "platform: $(/bin/kobo_config.sh 2>/dev/null)"

section "framebuffer"
# The host derives orientation from the visible raster, not from var.rotate, so
# xres/yres are what matter here.
for attribute in virtual_size bits_per_pixel rotate stride name; do
    [ -r "/sys/class/graphics/fb0/$attribute" ] &&
        echo "$attribute: $(cat "/sys/class/graphics/fb0/$attribute")"
done
command -v fbset >/dev/null 2>&1 && fbset -i

section "remote"
# What a takeover boot needs to be reachable, and what nickel used to provide.
grep -q " /dev/pts " /proc/mounts 2>/dev/null &&
    echo "devpts: mounted" || echo "devpts: NOT mounted"
[ -e /dev/ptmx ] && echo "ptmx: present" || echo "ptmx: MISSING"
# Started as a busybox applet the process is named busybox, so pidof telnetd
# says no while you are reading this over it. Ask who holds the port.
awk '$2 ~ /:0017$/ && $4 == "0A" { found = 1 } END { exit !found }' \
    /proc/net/tcp 2>/dev/null &&
    echo "telnet: port served" || echo "telnet: port NOT served"

# Every writer to the hardware-status pipe parks here forever when nobody is
# reading it, which is what happens once nickel is gone. Names, not a count:
# this session and its own pipelines are in the list too.
echo "parked on a pipe:"
for entry in /proc/[0-9]*; do
    [ "$(cat "$entry/wchan" 2>/dev/null)" = pipe_wait ] || continue
    echo "  ${entry#/proc/} $(tr '\0' ' ' <"$entry/cmdline" 2>/dev/null)"
done

section "input"
# Names, handlers and ABS bitmaps for every node. The host picks the first node
# exposing MT X/Y, or ABS_X/ABS_Y together with BTN_TOUCH.
cat /proc/bus/input/devices

section "nickel"
if pidof nickel >/dev/null 2>&1; then
    echo "running (pid $(pidof nickel))"
else
    echo "stopped"
fi

section "host probe"
if [ -x "$POCKETJS_BIN" ]; then
    "$POCKETJS_BIN" --probe 2>&1
else
    echo "host binary not deployed at $POCKETJS_BIN"
fi

echo
echo "Touch axes are not settled by this report. Run the live probe next:"
echo "  $POCKETJS_BIN --probe-touch"
