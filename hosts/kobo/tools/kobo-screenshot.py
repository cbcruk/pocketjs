#!/usr/bin/env python3
"""Photograph the device's screen without photographing the device.

    tools/kobo-screenshot.py <ip> <out.pgm> [--png]

A camera phone gives a picture of a room with an e-reader in it. This reads
/dev/fb0 over telnet instead, undoes the panel rotation, and writes the pixels
the app actually drew — which is what a bug report needs.

The framebuffer is 1.5 MB of Rgb565, and telnet is a text channel, so it comes
back base64-encoded. Big transfers while the panel is live have frozen this
device before (HANDOVER §4-5-1); one screen is fine, a loop of them is not.
"""

import base64
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from kobo_telnet import ShellError, run

# The Glo's panel, landscape as the controller reports it; the host renders
# portrait and maps through R270 (hosts/kobo/src/geometry.rs).
PANEL_W, PANEL_H, STRIDE = 1024, 758, 2048
RENDER_W, RENDER_H = 758, 1024


def gray565(value: int) -> int:
    red = (value >> 11) & 0x1F
    green = (value >> 5) & 0x3F
    blue = value & 0x1F
    return (
        ((red * 527 + 23) >> 6)
        + ((green * 259 + 33) >> 6)
        + ((blue * 527 + 23) >> 6)
    ) // 3


def main() -> int:
    if len(sys.argv) < 3:
        sys.stderr.write(__doc__.split("\n\n", 1)[1].split("\n\n")[0] + "\n")
        return 2
    host, out = sys.argv[1], Path(sys.argv[2])

    command = (
        f"dd if=/dev/fb0 bs={STRIDE} count={PANEL_H} 2>/dev/null | "
        "openssl base64 | tr -d '\\n'"
    )
    try:
        encoded = run(host, command, 240.0)
    except ShellError as error:
        sys.stderr.write(f"kobo-screenshot: {error}\n")
        return 1

    raw = base64.b64decode(encoded)
    expected = STRIDE * PANEL_H
    if len(raw) != expected:
        sys.stderr.write(
            f"kobo-screenshot: got {len(raw)} bytes, expected {expected}; "
            "the read was cut short\n"
        )
        return 1

    image = bytearray(RENDER_W * RENDER_H)
    for panel_y in range(PANEL_H):
        row = raw[panel_y * STRIDE : panel_y * STRIDE + PANEL_W * 2]
        # R270 maps render (x, y) to panel (y, render_w - 1 - x), so this is
        # that read backwards. Getting it wrong gives a picture that looks
        # plausible and is upside down.
        render_x = RENDER_W - 1 - panel_y
        base = render_x
        for panel_x in range(PANEL_W):
            value = row[panel_x * 2] | (row[panel_x * 2 + 1] << 8)
            image[panel_x * RENDER_W + base] = gray565(value)

    out.write_bytes(b"P5\n%d %d\n255\n" % (RENDER_W, RENDER_H) + bytes(image))
    print(f"kobo-screenshot: {out} — {RENDER_W}x{RENDER_H} Gray8")

    if "--png" in sys.argv:
        png = out.with_suffix(".png")
        # sips ships with macOS; elsewhere the .pgm opens in anything.
        if subprocess.run(
            ["sips", "-s", "format", "png", str(out), "--out", str(png)],
            capture_output=True,
        ).returncode == 0:
            print(f"kobo-screenshot: {png}")
        else:
            sys.stderr.write("kobo-screenshot: no sips; keeping the .pgm\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
