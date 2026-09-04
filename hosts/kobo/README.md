# PocketJS Kobo host

A standalone Rust binary that runs a PocketJS UI bundle on a Kobo e-reader.
The target is `kobo-glo`, host ABI 5: a 379×512 logical viewport at density 2,
which is exactly half the Glo's 758×1024 portrait panel on both axes. That
makes the Gray8 software rasterizer write a native-density surface with no
fractional scaling and no letterbox.

The host is derived from the `hosts/kindle` e-ink host: same raw-framebuffer
present layer, same damage/refresh policy, same evdev reader. Only the device
contract (geometry, touch protocol, launcher) is Kobo-specific.

## Why 379×512

The touch wire packs a contact as `(id << 18) | (y << 9) | x` — 9 bits per
axis, so the largest coordinate a host may report is 511. 512 is the viewport
EXTENT; the largest coordinate inside it is 511. A 379×512 viewport therefore
fits the legacy packing exactly, and the host never needs the wide-form
contact (`framework/src/touch.ts`, bit31=1, 10 bits per axis).

## Device profile

The checked-in contract is deliberately narrow, in the same spirit as the
Kindle host: the binary refuses geometry it has not been told about instead of
guessing.

- Kobo Glo (codename **Kraken**, 2012), Mark 4, i.MX507, 212 dpi;
- exactly a 758×1024 or rotated 1024×758 visible framebuffer;
- an infrared (Neonode) **single-contact** digitizer — FBInk records the Glo
  as `isKoboNonMT`, so it reports `ABS_X`/`ABS_Y` + `BTN_TOUCH` rather than the
  multitouch protocol. `input.rs` covers both; the Glo takes the non-MT branch.

A successful build is not evidence that another Kobo model, panel size,
framebuffer format, or touch controller works.

## Build

Kobo firmware is ARMv7 hard-float. The musl build keeps the deployed binary
independent of the firmware's glibc, which removes any need to pin a glibc
version at link time:

```sh
rustup target add armv7-unknown-linux-musleabihf
cargo install cargo-zigbuild
cargo test --manifest-path hosts/kobo/Cargo.toml
CARGO_ZIGBUILD_ZIG_PATH="$HOME/.local/zig/zig" \
CLANG_PATH=/usr/bin/clang \
LIBCLANG_PATH=/usr/lib/llvm-18/lib \
  cargo zigbuild \
  --manifest-path hosts/kobo/Cargo.toml \
  --release \
  --target armv7-unknown-linux-musleabihf
```

The ARM build enables `rquickjs`'s generated bindings because that crate ships
no pre-generated bindings for this musl triplet, so a working libclang is
required. The deployable binary is:

```text
hosts/kobo/target/armv7-unknown-linux-musleabihf/release/pocketjs-kobo
```

It should report `ELF 32-bit LSB executable, ARM, EABI5 … statically linked`
with the `hard-float ABI` flag set.

## Building an app

```sh
bun pocket compile --target kobo-glo \
  --manifest apps/paper-ink/pocket.json --project-root .
```

`apps/paper-ink` is the stock touch-only e-ink demo and the intended first
device check. `apps/hero` does NOT build for this target: it requires
`input.buttons`, and the Glo's only physical keys (power, frontlight) are
reserved by the firmware, so the profile advertises touch alone.

## Checking the render without hardware

The host writes `/dev/fb0` and refuses to run anywhere else, so the Gray8
pipeline is inspected through an example that stops one step earlier:

```sh
cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
  dist/paper-ink-main.js dist/paper-ink-main.pak /tmp/paper-ink
```

It writes `/tmp/paper-ink.pgm` at the full 758×1024 and exits nonzero if the
frame came out uniform — a bundle that boots but paints nothing otherwise
looks identical to a working one in the logs.

## Getting a shell

Everything below needs a root shell on the device. Two routes, in order of how
little they change:

1. **Firmware debug services.** Mount the Kobo over USB and append to
   `.kobo/Kobo/Kobo eReader.conf`:

   ```ini
   [DeveloperSettings]
   EnableDebugServices=true
   ForceWifiOn=true
   ```

   Eject, reboot, join Wi-Fi, read the address off *Settings → Device
   information*, then `telnet <ip>` and log in as `root` with an empty
   password. Deleting the two lines undoes it.
2. **KOReader.** Its *Tools → SSH* server is a real SSH daemon, and the same
   install directory carries the FBInk CLI this host needs (see below), so one
   install covers both prerequisites.

## Running on the device

```sh
pocketjs-kobo --js app.js --pak app.pak [options]
pocketjs-kobo --probe            # report fb geometry, format and touch nodes
pocketjs-kobo --probe-touch      # report live touch coordinates
```

| Option | Env | Default |
| --- | --- | --- |
| `--framebuffer PATH` | `POCKETJS_FRAMEBUFFER` | `/dev/fb0` |
| `--fbink PATH` | `POCKETJS_FBINK` | `/mnt/onboard/.apps/pocketjs/bin/fbink` |
| `--present-hz N` | `POCKETJS_PRESENT_HZ` | 30 |
| `--motion-waveform DU\|A2` | `POCKETJS_MOTION_WAVEFORM` | `DU` |
| `--ghost-budget N` | `POCKETJS_GHOST_BUDGET` | 80 |
| `--rotation auto\|0\|90\|180\|270` | `POCKETJS_ROTATION` | `auto` |

Neither probe writes the framebuffer, so both are safe to run while nickel owns
the panel. `--probe-touch` does claim the digitizer exclusively, so a probe tap
cannot also page the Kobo UI underneath it.

`SIGHUP` reloads JS/pak at the next 60 Hz frame boundary; `SIGINT`/`SIGTERM`
exit cleanly after a final refresh.

### FBInk is a runtime dependency, not a linked library

The host writes pixels itself and shells out to an independently installed
FBInk CLI for the panel update, so FBInk's per-generation Kobo mxcfb quirks
never have to be open-coded here. Install FBInk on the device and point
`--fbink` at it.

FBInk publishes source tarballs only, and building one means standing up
[koxtoolchain](https://github.com/koreader/koxtoolchain) first. The shortcut is
that a KOReader install already ships a Kobo-native `fbink` binary in its own
directory — `koreader.sh` drives the panel with it — so
`/mnt/onboard/.adds/koreader/fbink` is the first path `device/pocketjs.sh`
looks for.

The host issues:

```sh
fbink -q -s top=<y>,left=<x>,width=<w>,height=<h> -W <AUTO|DU|A2|GC16> [-f]
```

**Kobo-specific caveat to verify on device:** FBInk documents that on Kobo the
`-s` rectangle is *passed as-is to the ioctl, with no viewport or rotation
quirks applied*. The host computes that rectangle in the same coordinate space
it uses to write `/dev/fb0`, so the two should agree — but this is the first
thing to check if refreshes land in the wrong place.

### Device scripts

The host refuses to write `/dev/fb0` unless `POCKETJS_GUI_PAUSED=1` is set (or
`--allow-active-gui` is passed) so it cannot fight the Kobo UI for the panel.
It also takes `EVIOCGRAB` on the touchscreen so a tap cannot be delivered to
both runtimes. `device/` carries the three scripts that satisfy that contract:

| Script | What |
| --- | --- |
| `pocketjs.sh` | Pauses nickel, runs the host, restores nickel on **every** exit path |
| `nickel.sh` | `stop` / `start` / `status` on their own, for probes and recovery |
| `diagnose.sh` | Read-only device report; changes nothing, stops nothing |

Deploy them next to the binary and the bundle:

```text
/mnt/onboard/.apps/pocketjs/
  pocketjs-kobo   app.js   app.pak
  pocketjs.sh     nickel.sh   diagnose.sh
```

```sh
./pocketjs.sh                                  # defaults
./pocketjs.sh --motion-waveform A2 --ghost-budget 40   # extra args reach the host
killall -HUP pocketjs-kobo                     # reload the bundle in place
```

`pocketjs.sh` copies itself and `nickel.sh` to tmpfs and re-execs there before
it touches anything, because `/mnt/onboard` is FAT32 and vanishes the moment
the Kobo is plugged into a computer — if the launcher's own text went with it,
nickel would never come back. The restore runs from an `EXIT` trap, so a crash,
a `kill`, or a failed bundle all still return the UI.

The restart sequence follows KOReader's `platform/kobo/nickel.sh`. It
deliberately leaves Wi-Fi alone where KOReader tears it down: the development
loop runs over that interface.

`hosts/kobo/tests/device-scripts.sh` drives all of this against a staged root
with stubbed firmware tools, so the restore path is checkable without a Kobo:

```sh
hosts/kobo/tests/device-scripts.sh
```

## Device tuning checklist

These need the actual hardware and are not settled by this checkout:

1. **Framebuffer** — `--probe` and confirm 758×1024 (or 1024×758) and the
   reported bpp/rotation. Kobo's `mxc_epdc` driver reports the orientation
   nickel last set, which is why `geometry.rs` treats `var.rotate` as
   informational and derives orientation from the exact visible raster.
2. **Touch axes** — `--probe-touch`, then set the calibration env vars. It
   prints every contact as raw evdev values, panel pixels and logical
   coordinates at once, so tapping the four corners settles the mapping
   directly instead of inferring it from `evtest`. The runtime swaps the axes
   before it mirrors them, so settle the swap, re-run, then decide the flips.
   FBInk's device table records the Glo as `touchSwapAxes=true` and
   `touchMirrorX=true`, i.e. expect to need
   `POCKETJS_TOUCH_SWAP_XY=1 POCKETJS_TOUCH_FLIP_X=1`. Confirm, do not assume.
3. **Waveforms** — A2/DU/GC16 are all available on the Glo's Pearl panel.
   Tune `--motion-waveform` and `--ghost-budget` against real ghosting.
4. **Idle** — verify the quiet-period GC16 cleanup and the periodic full
   flash behave over a long session.
