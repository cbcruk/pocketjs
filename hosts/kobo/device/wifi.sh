#!/bin/sh
# Bring the Kobo's Wi-Fi up without nickel.
#
#   wifi.sh up | down | status
#
# Nothing in the firmware does this: rcS only exports where the modules live,
# and nickel itself is what loads them, starts wpa_supplicant and runs the DHCP
# client. Stop nickel for long enough and the network is nobody's job — which
# is the first thing that has to change before a PocketJS session can outlive
# the UI it replaced.
#
# The sequence is Nickel's, read off a running device and cross-checked against
# KOReader's platform/kobo/enable-wifi.sh:
#
#   sdio_wifi_pwr.ko -> dhd.ko -> ifconfig up -> wlarm_le up
#   -> wpa_supplicant -D wext -> dhcpcd
#
# Credentials are not this script's business. Nickel already wrote the joined
# network into /etc/wpa_supplicant/wpa_supplicant.conf; this only starts the
# daemons that read it. A device that has never joined a network from the Kobo
# UI has nothing here to bring up.

set -u

WIFI_ROOT="${WIFI_ROOT:-}"
WIFI_IFACE="${WIFI_IFACE:-${INTERFACE:-eth0}}"
WIFI_MODULE="${WIFI_MODULE:-dhd}"
WIFI_SUPPLICANT_DRIVER="${WIFI_SUPPLICANT_DRIVER:-wext}"
# In 250ms ticks.
WIFI_DHCP_TIMEOUT="${WIFI_DHCP_TIMEOUT:-60}"

# Where the firmware keeps its Wi-Fi modules, asked rather than guessed.
#
# rcS exports WIFI_MODULE_PATH on every release seen so far, and it is the only
# answer that survives them: 2.1.5 keeps the modules under /drivers/ntx508 and
# does not export PLATFORM at all, while 3.19 exports PLATFORM=mx50-ntx.
# Building the path from a platform name found neither.
wifi_module_dir() {
    if [ -n "${WIFI_MODULE_PATH:-}" ] && [ -e "$WIFI_ROOT$WIFI_MODULE_PATH" ]; then
        dirname "$WIFI_ROOT$WIFI_MODULE_PATH"
        return 0
    fi
    for dir in "$WIFI_ROOT"/drivers/*/wifi; do
        if [ -e "$dir/$WIFI_MODULE.ko" ]; then
            echo "$dir"
            return 0
        fi
    done
    return 1
}

wifi_supplicant_conf() {
    # wifi.conf next to the launcher wins. Without it the device joins
    # whatever the file nickel wrote ranks highest, which on a takeover boot
    # is a decision nobody can see being made and nobody can change: two boots
    # came up with the clock on screen, the radio on some other network, and
    # no way in to ask why. This file sits on the FAT32 partition, so a card
    # reader and any text editor are enough to pin the network — no ext4
    # tools, and no working network needed to fix the network.
    #
    # It is a wpa_supplicant.conf. The smallest useful one:
    #
    #   network={
    #       ssid="my-network"
    #       psk="my-password"
    #   }
    #
    # FW 5.x moved nickel's copy onto the user partition; this one is still on
    # rootfs, so both are tried after ours.
    for candidate in \
        "$WIFI_ROOT${POCKETJS_DIR:-/mnt/onboard/.apps/pocketjs}/wifi.conf" \
        "$WIFI_ROOT/mnt/onboard/.kobo/wpa_supplicant.conf" \
        "$WIFI_ROOT/etc/wpa_supplicant/wpa_supplicant.conf"; do
        [ -f "$candidate" ] && { echo "$candidate"; return 0; }
    done
    return 1
}

# An address alone does not mean a working interface: ifconfig keeps reporting
# one after `ifconfig <iface> down`, which is how an earlier version of this
# script reported success while leaving wpa_supplicant stopped.
wifi_address() {
    state="$(ifconfig "$WIFI_IFACE" 2>/dev/null)" || return 0
    echo "$state" | grep -q '\bUP\b' || return 0
    echo "$state" | sed -n 's/.*inet addr:\([0-9.]*\).*/\1/p'
}

module_loaded() {
    grep -q "^$1 " "$WIFI_ROOT/proc/modules" 2>/dev/null
}

insmod_as_needed() {
    module_loaded "$1" && return 0
    dir="$(wifi_module_dir)" || {
        echo "wifi: found no /drivers/*/wifi holding $WIFI_MODULE.ko" >&2
        return 1
    }
    file="$dir/$1.ko"
    [ -e "$file" ] || {
        echo "wifi: no module at $file" >&2
        return 1
    }
    insmod "$file" || return 1
    usleep 250000
}

# Every step is a no-op when it is already satisfied, so this doubles as the
# repair path: run it against a half-torn-down interface and it finishes the
# job. Short-circuiting on any single signal is what hid a stopped supplicant.
wifi_up() {
    insmod_as_needed sdio_wifi_pwr || return 1
    usleep 250000
    insmod_as_needed "$WIFI_MODULE" || return 1
    # Racy as hell; nickel sleeps here too and so does KOReader.
    sleep 1

    ifconfig "$WIFI_IFACE" up || return 1
    [ "$WIFI_MODULE" = "dhd" ] && wlarm_le -i "$WIFI_IFACE" up

    if ! pidof wpa_supplicant >/dev/null 2>&1; then
        conf="$(wifi_supplicant_conf)" || {
            echo "wifi: no wpa_supplicant.conf; join a network from the Kobo UI first" >&2
            return 1
        }
        wpa_supplicant -D "$WIFI_SUPPLICANT_DRIVER" -i "$WIFI_IFACE" \
            -c "$conf" -C /var/run/wpa_supplicant -B || return 1
    fi

    if [ -z "$(wifi_address)" ] || ! pidof dhcpcd udhcpc >/dev/null 2>&1; then
        if [ -x /sbin/dhcpcd ]; then
            dhcpcd -d -t 30 -w "$WIFI_IFACE"
        else
            udhcpc -S -i "$WIFI_IFACE" -s /etc/udhcpc.d/default.script -b -q
        fi
    fi

    ticks=0
    while [ -z "$(wifi_address)" ]; do
        if [ "$ticks" -ge "$WIFI_DHCP_TIMEOUT" ]; then
            echo "wifi: no address after $((WIFI_DHCP_TIMEOUT / 4))s" >&2
            return 1
        fi
        usleep 250000
        ticks=$((ticks + 1))
    done
    echo "wifi: up on $WIFI_IFACE ($(wifi_address))"
}

wifi_is_up() {
    [ -n "$(wifi_address)" ] && pidof wpa_supplicant >/dev/null 2>&1
}

wifi_down() {
    # Leaves the modules loaded: unloading them is where the SDIO bus gets
    # upset, and nickel reloads them itself when it comes back.
    killall -q -TERM dhcpcd udhcpc wpa_supplicant 2>/dev/null
    usleep 250000
    # Clearing the address as well as downing the link, so `status` and the
    # bring-up path see an interface that is actually unconfigured.
    ifconfig "$WIFI_IFACE" 0.0.0.0 2>/dev/null
    ifconfig "$WIFI_IFACE" down 2>/dev/null
    echo "wifi: down"
}

wifi_status() {
    address="$(wifi_address)"
    echo "interface  $WIFI_IFACE ${address:-(no address)}"
    echo "modules    sdio_wifi_pwr=$(module_loaded sdio_wifi_pwr && echo yes || echo no) \
$WIFI_MODULE=$(module_loaded "$WIFI_MODULE" && echo yes || echo no)"
    echo "module dir $(wifi_module_dir || echo '(not found)')"
    echo "config     $(wifi_supplicant_conf || echo '(none found)')"
    echo "supplicant $(pidof wpa_supplicant >/dev/null 2>&1 && echo running || echo stopped)"
    echo "dhcp       $(pidof dhcpcd >/dev/null 2>&1 || pidof udhcpc >/dev/null 2>&1 &&
        echo running || echo stopped)"
    wifi_is_up
}

if [ "${WIFI_SH_LIBRARY:-0}" != "1" ]; then
    case "${1:-}" in
        up) wifi_up ;;
        down) wifi_down ;;
        status) wifi_status ;;
        *)
            echo "usage: wifi.sh up | down | status" >&2
            exit 2
            ;;
    esac
fi
