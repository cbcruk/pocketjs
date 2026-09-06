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
   ```

   Eject, reboot, join Wi-Fi, read the address off *Settings → Device
   information*, then `telnet <ip>` and log in as `root` with an empty
   password. Deleting the two lines undoes it.

   **Do not add `ForceWifiOn=true`.** It is the obvious way to stop a
   development session losing the network it arrived on, and it also keeps the
   device talking to Kobo. One left overnight took a firmware update spanning
   a decade, came back with a garbled panel and an unresponsive digitizer, and
   eventually factory-restored itself. Turn the radio on by hand instead.
2. **KOReader.** Its *Tools → SSH* server is a real SSH daemon, which is a
   nicer channel than telnet and does not depend on the firmware's debug
   services being what you expect.

## Running on the device

```sh
pocketjs-kobo --js app.js --pak app.pak [options]
pocketjs-kobo --probe            # report fb geometry, format and touch nodes
pocketjs-kobo --probe-touch      # report live touch coordinates
```

| Option | Env | Default |
| --- | --- | --- |
| `--framebuffer PATH` | `POCKETJS_FRAMEBUFFER` | `/dev/fb0` |
| `--present-hz N` | `POCKETJS_PRESENT_HZ` | 30 |
| `--motion-waveform DU\|A2` | `POCKETJS_MOTION_WAVEFORM` | `DU` |
| `--ghost-budget N` | `POCKETJS_GHOST_BUDGET` | 80 |
| `--rotation auto\|0\|90\|180\|270` | `POCKETJS_ROTATION` | `auto` |
| `--sim-hz N` | `POCKETJS_SIM_HZ` | 30 |

`--sim-hz` is the virtual frame rate published to the guest as `__simHz`, and
it must divide 60. Simulating faster than the panel can present is provably
wasted work: measured on a Glo, the cost is linear in the rate and per-tick
cost is flat.

| `--sim-hz` | CPU of one core | per tick |
| --- | --- | --- |
| 60 | 14.5% | 2.2 ms |
| 30 | 7% | 2.2 ms |
| 10 | 2% | 2.3 ms |

30 is the default because the panel presents at 30 Hz at best. An app whose
screen changes on a human timescale rather than a frame one — a clock, a
status board — should go lower; touch registers within one frame either way,
and a DU waveform takes longer to settle than 100 ms.

`POCKETJS_PROFILE_SECS=N` logs where each tick's time actually goes, including
how many of those ticks repainted anything.

A tick whose draw list is byte-identical to the last one repaints nothing: a
`DrawList` is one flat `Vec<u32>`, so the check is a memcmp, and the retained
raster and the caller's pending damage both still stand. Frames are still
ticked on schedule — virtual time is a frame counter, so dropping one stops
the guest's clock — they are just cheap. Measured on a Glo running the clock:

| | CPU of one core |
| --- | --- |
| 60 Hz, repainting every tick | 14.5% |
| 30 Hz | 7% |
| 30 Hz, skipping unchanged frames | 4% |

What is left is `ui.draw()` rebuilding the list every tick, about 1.1 ms of it.
That is engine-side and shared with every other host. Skipping the `words`
clone on idle ticks was tried and measured nothing, so it is not in here.

### The power key

Nickel normally owns the power button, so with nickel paused a press does
nothing — which is why a running session can look like a hung device. The host
reads `KEY_POWER` off the vestigial keypad node (`mxckpd`, separate from the
digitizer) and acts on release, because there is no way to tell the user what a
hold is about to do while it is happening:

| Press | Effect |
| --- | --- |
| Short | `power.sh suspend`, then a reload |
| Held 1.5s or more | Exit, so the launcher's trap hands nickel back |

Verified on a Glo: the host finds the key on `/dev/input/event0` and a long
press hands the device back. The key reads as nothing while nickel runs —
nickel holds `EVIOCGRAB` on that node — which is only ever the case when this
host is not running.

**Suspend on this model is not settled.** Nickel's own short press is a display
sleep, not a suspend to RAM: the network stays up and `/proc/uptime` keeps
counting through it. `power.sh` asks for `mem`, which is deeper than anything
nickel does here, and one attempt did not come back. Point `--power-helper` at
something else, or pass `--no-power-key`, until that is understood.

The reload after a resume is not incidental. Virtual time is a frame counter,
so it does not advance while the machine is down; republishing the boot clock
is the only way a calendar app comes back showing the right hour.

`--no-power-key` opts out. Sleeping is device policy rather than rendering, so
it lives in a shell script for the same reason the panel update does.

Neither probe writes the framebuffer, so both are safe to run while nickel owns
the panel. `--probe-touch` does claim the digitizer exclusively, so a probe tap
cannot also page the Kobo UI underneath it.

`SIGHUP` reloads JS/pak at the next 60 Hz frame boundary; `SIGINT`/`SIGTERM`
exit cleanly after a final refresh.

### The panel update is an ioctl, not a helper

This host used to shell out to an installed FBInk CLI, on the reasoning that
FBInk carries Kobo's per-generation mxcfb quirks so the host need not. That
convenience had a price and it came due: FBInk is a hard-float binary, the
Glo's 2012 firmware ships a soft-float userspace, and a static-musl host that
needs nothing at all could not start it. One dynamic dependency tied the whole
runtime to a particular firmware.

The update now goes straight to the driver. There is one device to support and
its interface is fixed — a Mark 4 i.MX50 on the NTX 2.6.35 kernel, taking
`mxcfb_update_data_v1_ntx` on `MXCFB_SEND_UPDATE`. The constants are
transcribed from FBInk's `eink/mxcfb-kobo.h`, which remains the reference for
what they mean; a test recomputes the `_IOW` encodings so a mistyped one fails
the build rather than the panel, and a `const` assertion pins the struct at the
68 bytes the ioctl number encodes.

Nothing needs installing on the device beyond this binary and the bundle.

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
| `wifi.sh` | `up` / `down` / `status` — the network without nickel |
| `power.sh` | `suspend` / `status` — sleep, and put the device back together |

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

The restart sequence follows KOReader's `platform/kobo/nickel.sh`, and restores
the environment `/etc/init.d/rcS` gives nickel — a shell that arrived over
telnet has none of it, and a nickel restarted without `WIFI_MODULE_PATH`
cannot reload the radio.

It also leaves the DHCP client running where KOReader kills it. KOReader can
afford to: it brings Wi-Fi up itself. `dhcpcd` deconfigures its interface and
releases the lease on `SIGTERM`, so killing it is exactly how a session drops
off the network it arrived on. The clients are handed back at restart, since
nickel expects to own them.

Nothing in the firmware brings Wi-Fi up — rcS only exports where the modules
live, and nickel is what loads them. `wifi.sh` does that job for a session that
outlives the UI:

```sh
./wifi.sh status          # interface, modules, supplicant, dhcp
./wifi.sh up              # idempotent, and doubles as the repair path
```

Credentials are not its business: nickel already wrote the joined network into
`/etc/wpa_supplicant/wpa_supplicant.conf`, and this only starts the daemons
that read it. A device that has never joined a network from the Kobo UI has
nothing to bring up.

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
