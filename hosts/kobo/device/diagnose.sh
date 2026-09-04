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

section "fbink"
for candidate in \
    "$POCKETJS_DIR/bin/fbink" \
    /mnt/onboard/.adds/koreader/fbink \
    /usr/local/bin/fbink; do
    if [ -x "$candidate" ]; then
        echo "found: $candidate"
        "$candidate" -e 2>&1 | tr ';' '\n'
        break
    fi
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
