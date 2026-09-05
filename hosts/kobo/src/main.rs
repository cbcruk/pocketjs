//! PocketJS UI runtime for Kobo e-readers.

mod damage;
mod framebuffer;
mod geometry;
mod input;
mod refresh;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use damage::{DamageTracker, Rect};
use framebuffer::Framebuffer;
use geometry::{Geometry, Rotation, compatible_reported_rotation};
use input::{ContactReport, Input, PowerKey};
use pocket_mod::Guest;
use pocket_ui_surface::UiSurface;
use pocketjs_core::spec;
use refresh::{FbInk, RefreshPolicy, Waveform};
use signal_hook::consts::signal::{SIGHUP, SIGINT, SIGTERM};

const HOST_ID: &str = "kobo-glo";
const HOST_ABI: u32 = 5;
const LOGICAL_W: usize = 379;
const LOGICAL_H: usize = 512;
const DENSITY: usize = 2;
/// Poll cadence for `--probe-touch`, which has no simulation to pace.
const POLL_TICK: Duration = Duration::from_nanos(16_666_667);
const MAX_CATCHUP_TICKS: usize = 4;
/// Held at least this long, the power key means "give me the device back"
/// rather than "sleep". Judged on release, so a hold is never ambiguous while
/// it is happening — there is no way to tell the user what it is about to do.
const POWER_LONG_PRESS: Duration = Duration::from_millis(1500);
/// Core ticks a bundle's realm advances per virtual second (spec FIXED_DT).
const CORE_TICKS_PER_SECOND: u32 = 60;

/// Virtual frames per second, published to the guest as `__simHz`.
///
/// This is a host policy, not app code (docs/DETERMINISM.md), and 60 is the
/// wrong policy for e-ink: the panel presents at 30 Hz at best and a DU
/// waveform takes longer than that to settle, so most of those frames can
/// never reach the glass. It has to divide the core tick rate exactly.
fn parse_sim_hz(value: &str) -> Result<u32> {
    let hz: u32 = value.parse().context("--sim-hz must be an integer")?;
    if hz == 0 || !CORE_TICKS_PER_SECOND.is_multiple_of(hz) {
        bail!(
            "--sim-hz must divide {CORE_TICKS_PER_SECOND} exactly \
             (1, 2, 3, 4, 5, 6, 10, 12, 15, 20, 30 or 60); got {hz}"
        );
    }
    Ok(hz)
}

#[derive(Debug)]
struct Args {
    js: PathBuf,
    pak: PathBuf,
    framebuffer: PathBuf,
    fbink: PathBuf,
    present_hz: u32,
    motion_waveform: Waveform,
    ghost_budget: u32,
    rotation: Option<Rotation>,
    sim_hz: u32,
    power_helper: Option<PathBuf>,
    probe: bool,
    probe_touch: bool,
    allow_active_gui: bool,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut args = Self {
            js: env_path("POCKET_JS").unwrap_or_else(|| "app.js".into()),
            pak: env_path("POCKET_PAK").unwrap_or_else(|| "app.pak".into()),
            framebuffer: env_path("POCKETJS_FRAMEBUFFER").unwrap_or_else(|| "/dev/fb0".into()),
            fbink: env_path("POCKETJS_FBINK")
                .unwrap_or_else(|| "/mnt/onboard/.apps/pocketjs/bin/fbink".into()),
            present_hz: env_parse("POCKETJS_PRESENT_HZ")?.unwrap_or(30),
            motion_waveform: Waveform::parse_motion(
                &std::env::var("POCKETJS_MOTION_WAVEFORM").unwrap_or_else(|_| "DU".into()),
            )?,
            ghost_budget: env_parse("POCKETJS_GHOST_BUDGET")?.unwrap_or(80),
            rotation: Rotation::parse(
                &std::env::var("POCKETJS_ROTATION").unwrap_or_else(|_| "auto".into()),
            )?,
            sim_hz: match std::env::var("POCKETJS_SIM_HZ") {
                Ok(value) => parse_sim_hz(&value)?,
                Err(_) => 30,
            },
            power_helper: Some(
                env_path("POCKETJS_POWER_HELPER")
                    .unwrap_or_else(|| "/mnt/onboard/.apps/pocketjs/power.sh".into()),
            ),
            probe: false,
            probe_touch: false,
            allow_active_gui: false,
        };

        let words = std::env::args().skip(1).collect::<Vec<_>>();
        let mut index = 0;
        while index < words.len() {
            let word = &words[index];
            let value = |index: &mut usize| -> Result<&str> {
                *index += 1;
                words
                    .get(*index)
                    .map(String::as_str)
                    .ok_or_else(|| anyhow::anyhow!("{word} requires a value"))
            };
            match word.as_str() {
                "--js" => args.js = value(&mut index)?.into(),
                "--pak" => args.pak = value(&mut index)?.into(),
                "--framebuffer" => args.framebuffer = value(&mut index)?.into(),
                "--fbink" => args.fbink = value(&mut index)?.into(),
                "--present-hz" => {
                    args.present_hz = value(&mut index)?
                        .parse()
                        .context("--present-hz must be an integer")?
                }
                "--motion-waveform" => {
                    args.motion_waveform = Waveform::parse_motion(value(&mut index)?)?
                }
                "--ghost-budget" => {
                    args.ghost_budget = value(&mut index)?
                        .parse()
                        .context("--ghost-budget must be an integer")?
                }
                "--rotation" => args.rotation = Rotation::parse(value(&mut index)?)?,
                "--sim-hz" => args.sim_hz = parse_sim_hz(value(&mut index)?)?,
                "--power-helper" => args.power_helper = Some(value(&mut index)?.into()),
                "--no-power-key" => args.power_helper = None,
                "--probe" => args.probe = true,
                "--probe-touch" => args.probe_touch = true,
                "--allow-active-gui" => args.allow_active_gui = true,
                "-h" | "--help" => {
                    print_help();
                    std::process::exit(0);
                }
                _ => bail!("unknown argument {word:?}; use --help"),
            }
            index += 1;
        }
        Ok(args)
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn env_parse<T>(name: &str) -> Result<Option<T>>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    std::env::var(name)
        .ok()
        .map(|value| {
            value
                .parse()
                .map_err(|error| anyhow::anyhow!("{name}={value:?}: {error}"))
        })
        .transpose()
}

fn print_help() {
    println!(
        "\
PocketJS Kobo host

Usage:
  pocketjs-kobo --js app.js --pak app.pak [options]
  pocketjs-kobo --probe [options]
  pocketjs-kobo --probe-touch [options]

Options:
  --framebuffer PATH       Linux framebuffer (default /dev/fb0)
  --fbink PATH             external FBInk CLI
  --present-hz N           physical refresh cap, 1..60 (default 30)
  --motion-waveform DU|A2  fast shallow-refresh waveform (default DU)
  --ghost-budget N         fast updates before a full GC16 cleanup
  --rotation auto|0|90|180|270
  --sim-hz N               virtual frames per second, must divide 60 (default 30)
  --power-helper PATH      script run as `PATH suspend` on a short power press
  --no-power-key           ignore the power key entirely
  --probe                  report framebuffer geometry and exit
  --probe-touch            report live touch coordinates until interrupted
  --allow-active-gui       explicit unsafe override of the nickel-pause guard

Set POCKETJS_PROFILE_SECS=N to log where each logic tick's time goes.

SIGHUP reloads JS/pak at the next 60Hz frame boundary. SIGINT/SIGTERM exit.
The matching environment variables are POCKET_JS, POCKET_PAK,
POCKETJS_FRAMEBUFFER, POCKETJS_FBINK, POCKETJS_PRESENT_HZ,
POCKETJS_MOTION_WAVEFORM, POCKETJS_GHOST_BUDGET, POCKETJS_ROTATION and
POCKETJS_SIM_HZ.
Touch calibration is environment-only and applied in this order:
POCKETJS_TOUCH_SWAP_XY, then POCKETJS_TOUCH_FLIP_X and POCKETJS_TOUCH_FLIP_Y.
POCKETJS_TOUCH_{{X,Y}}_{{MIN,MAX}} replace the axis extents the digitizer declares,
which a driver may advertise without using. Use --probe-touch to settle them."
    );
}

/// Report touch contacts in raw, panel and logical coordinates until the
/// operator interrupts.
///
/// This never writes the framebuffer, so it is safe to run while nickel owns
/// the panel. It does claim the digitizer exclusively, so a probe tap cannot
/// also page the Kobo UI underneath.
fn probe_touch(input: &mut Input, geometry: &Geometry) -> Result<()> {
    input
        .grab_selected()
        .context("claiming the Kobo touchscreen")?;
    let Some(axes) = input.selected_axes() else {
        bail!("touchscreen disappeared between the grab and the probe");
    };

    let max_x = geometry.logical_w - 1;
    let max_y = geometry.logical_h - 1;
    println!("PocketJS Kobo touch probe");
    println!(
        "  node         {} ({})",
        axes.path,
        if axes.multitouch {
            "multitouch"
        } else {
            "single-contact"
        }
    );
    println!(
        "  raw axes     x={}..{}  y={}..{}",
        axes.x_min, axes.x_max, axes.y_min, axes.y_max
    );
    println!(
        "  panel        {}x{} ({:?})",
        geometry.panel_w, geometry.panel_h, geometry.rotation
    );
    println!(
        "  logical      {}x{}, largest coordinate ({max_x}, {max_y})",
        geometry.logical_w, geometry.logical_h
    );
    println!("  calibration  {}", input.calibration_summary());
    println!();
    println!("Hold the device upright and tap each corner. Expected logical values:");
    println!("  top-left      (0, 0)");
    println!("  top-right     ({max_x}, 0)");
    println!("  bottom-left   (0, {max_y})");
    println!("  bottom-right  ({max_x}, {max_y})");
    println!();
    println!("If the two logical axes are exchanged, set POCKETJS_TOUCH_SWAP_XY=1.");
    println!("If logical x counts down where it should count up, set POCKETJS_TOUCH_FLIP_X=1.");
    println!("If logical y counts down where it should count up, set POCKETJS_TOUCH_FLIP_Y=1.");
    println!("The runtime swaps before it mirrors: settle the swap, re-run, then the flips.");
    println!();
    println!("Ctrl-C to stop.");

    let terminate = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGINT, terminate.clone()).context("registering SIGINT")?;
    signal_hook::flag::register(SIGTERM, terminate.clone()).context("registering SIGTERM")?;

    let mut previous = Vec::<ContactReport>::new();
    while !terminate.load(Ordering::Relaxed) {
        let reports = input.poll_reports(geometry)?;
        if reports != previous {
            if reports.is_empty() {
                println!("release");
            }
            for report in &reports {
                println!(
                    "slot={} raw=({}, {}) panel=({}, {}) logical=({}, {})",
                    report.slot,
                    report.raw_x,
                    report.raw_y,
                    report.panel_x,
                    report.panel_y,
                    report.logical_x,
                    report.logical_y
                );
            }
            previous = reports;
        }
        std::thread::sleep(POLL_TICK);
    }
    println!("touch probe stopped");
    Ok(())
}

/// Publish the device's local wall clock as a one-shot boot input.
///
/// PocketJS time is a frame counter by design (docs/DETERMINISM.md): no host
/// may hand the guest a live clock, and none does. A calendar still has to
/// start somewhere, so this writes the local time ONCE, into the same contract
/// slot the runtime already uses for `__simHz` and `__pak`, before the bundle
/// evals. Everything after it is `virtualNow()`, so replaying the same boot
/// value replays the same trajectory — the fold stays pure.
///
/// A SIGHUP reload runs this again, which is also how a long-running session
/// resynchronizes against the drift a dropped logic tick leaves behind.
fn publish_boot_clock(guest: &Guest) -> Result<()> {
    // SAFETY: time(NULL) returns the epoch and dereferences nothing.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut broken: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r writes one struct tm through a pointer we own, and
    // reads the time_t we just produced.
    if unsafe { libc::localtime_r(&now, &mut broken) }.is_null() {
        bail!("localtime_r failed for epoch {now}; is the device timezone readable?");
    }
    let second_of_day = broken.tm_hour * 3600 + broken.tm_min * 60 + broken.tm_sec;
    guest
        .eval(
            "boot-clock",
            &format!(
                "globalThis.__bootClock = {{ year: {}, month: {}, day: {}, \
                 weekday: {}, secondOfDay: {} }};",
                broken.tm_year + 1900,
                broken.tm_mon + 1,
                broken.tm_mday,
                broken.tm_wday,
                second_of_day
            ),
        )
        .context("publishing the boot clock")?;
    log::info!(
        "kobo boot clock: {:04}-{:02}-{:02} {:02}:{:02}:{:02} local (weekday {})",
        broken.tm_year + 1900,
        broken.tm_mon + 1,
        broken.tm_mday,
        broken.tm_hour,
        broken.tm_min,
        broken.tm_sec,
        broken.tm_wday
    );
    Ok(())
}

/// Where a logic tick's time actually goes.
///
/// The loop sleeps to its next deadline, so a busy core means the work inside
/// a tick is the cost, not the pacing. Guessing which half is expensive on a
/// 1 GHz ARM running a JS interpreter is how you optimize the wrong one.
#[derive(Default)]
struct Profile {
    ticks: u64,
    guest: Duration,
    raster: Duration,
    present: Duration,
}

impl Profile {
    fn drain(&mut self, window: Duration) -> String {
        let share = |part: Duration| part.as_secs_f64() * 100.0 / window.as_secs_f64().max(1e-9);
        let per_tick = |part: Duration| {
            if self.ticks == 0 {
                0.0
            } else {
                part.as_secs_f64() * 1e3 / self.ticks as f64
            }
        };
        let report = format!(
            "{} ticks in {:.1}s — guest {:.1}% ({:.2}ms/tick), raster {:.1}% ({:.2}ms/tick),              present {:.1}%",
            self.ticks,
            window.as_secs_f64(),
            share(self.guest),
            per_tick(self.guest),
            share(self.raster),
            per_tick(self.raster),
            share(self.present),
        );
        *self = Self::default();
        report
    }
}

/// Run the suspend helper and wait for the machine to come back.
///
/// Sleeping is device policy, not rendering, so it lives in a shell script for
/// the same reason the panel update does — the tested sequence stays in one
/// place and this host does not re-derive it. The call blocks: the system is
/// going down mid-call and returns from the same line on the way out.
fn suspend_through(helper: &std::path::Path) -> Result<()> {
    log::info!("kobo power: suspending via {}", helper.display());
    let status = std::process::Command::new(helper)
        .arg("suspend")
        .status()
        .with_context(|| format!("running {} suspend", helper.display()))?;
    if !status.success() {
        bail!("{} suspend exited with {status}", helper.display());
    }
    log::info!("kobo power: resumed");
    Ok(())
}

struct AppRuntime {
    guest: Guest,
    surface: UiSurface,
    damage: DamageTracker,
    profile: Profile,
}

impl AppRuntime {
    fn load(args: &Args, geometry: &Geometry) -> Result<Self> {
        let sim_hz = args.sim_hz;
        let pak = std::fs::read(&args.pak)
            .with_context(|| format!("reading pak {}", args.pak.display()))?;
        let js = std::fs::read_to_string(&args.js)
            .with_context(|| format!("reading bundle {}", args.js.display()))?;

        let surface = UiSurface::new_with_density(
            (geometry.logical_w as f32, geometry.logical_h as f32),
            geometry.density as u32,
        );
        surface.set_identity(HOST_ID, HOST_ABI);
        surface.feed_pak(&pak);
        let guest = Guest::new().context("creating PocketJS guest")?;
        surface.mount(&guest).context("mounting UI surface")?;
        // Same contract slot as `__pak`: host policy the bundle latches when it
        // mounts. Publishing it and then pacing the loop at a different rate
        // would make the guest's clock disagree with the wall.
        guest
            .eval("sim-hz", &format!("globalThis.__simHz = {sim_hz};"))
            .context("publishing the simulation rate")?;
        publish_boot_clock(&guest)?;
        guest.eval("app", &js).context("evaluating app bundle")?;
        if !guest.has_frame() {
            bail!(
                "{} installed no global frame(); was it built for {HOST_ID}?",
                args.js.display()
            );
        }
        Ok(Self {
            guest,
            surface,
            damage: DamageTracker::new(
                geometry.render_w,
                geometry.render_h,
                geometry.density as u32,
            ),
            profile: Profile::default(),
        })
    }

    fn tick(&mut self, touches: &[u32]) -> Result<Vec<Rect>> {
        let entered = Instant::now();
        self.guest
            .frame_with_touches(0, spec::ANALOG_CENTER, touches)
            .context("PocketJS guest frame")?;
        self.surface.tick();
        let rastering = Instant::now();
        self.profile.guest += rastering - entered;
        self.profile.ticks += 1;
        let damage = &mut self.damage;
        self.surface.with_ui(|ui| {
            let words = ui.draw().words.clone();
            damage.rasterize(ui, &words);
        });
        let dirty = self.damage.diff();
        self.profile.raster += rastering.elapsed();
        Ok(dirty)
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();
    let args = Args::parse()?;

    let mut framebuffer = Framebuffer::open(&args.framebuffer)?;
    let info = framebuffer.info().clone();
    log::info!(
        "kobo framebuffer: {} {}x{} virtual {}x{} +{},{} stride={} rotate={} {:?}",
        info.id,
        info.width,
        info.height,
        info.virtual_width,
        info.virtual_height,
        info.x_offset,
        info.y_offset,
        info.line_length,
        info.rotate,
        info.format
    );
    let render_w = LOGICAL_W * DENSITY;
    let render_h = LOGICAL_H * DENSITY;
    let requested_rotation = match args.rotation {
        Some(rotation) => Some(rotation),
        None => {
            let compatible = compatible_reported_rotation(
                info.rotate,
                render_w,
                render_h,
                info.width,
                info.height,
            )?;
            if compatible.is_none() {
                log::warn!(
                    "framebuffer rotate={} is incompatible with visible {}x{}; \
                     treating it as controller-applied and deriving orientation \
                     from the exact render dimensions",
                    info.rotate,
                    info.width,
                    info.height
                );
            }
            compatible
        }
    };
    let geometry = Geometry::exact(
        LOGICAL_W,
        LOGICAL_H,
        DENSITY,
        info.width,
        info.height,
        requested_rotation,
    )?;
    log::info!(
        "kobo geometry: {}x{} @{}x -> {}x{} {:?}",
        LOGICAL_W,
        LOGICAL_H,
        DENSITY,
        info.width,
        info.height,
        geometry.rotation
    );

    let mut input = Input::discover()?;
    log::info!("kobo input: {} touch device(s)", input.device_count());
    if args.probe {
        println!(
            "PocketJS Kobo probe OK: fb={} {}x{} {:?}, rotation={:?}, touch_devices={}",
            info.id,
            info.width,
            info.height,
            info.format,
            geometry.rotation,
            input.device_count()
        );
        return Ok(());
    }
    if args.probe_touch {
        return probe_touch(&mut input, &geometry);
    }

    let gui_paused = std::env::var("POCKETJS_GUI_PAUSED")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
    if !gui_paused && !args.allow_active_gui {
        bail!(
            "refusing to write /dev/fb0 while the Kobo GUI (nickel) may be active; \
             launch through the PocketJS device wrapper (POCKETJS_GUI_PAUSED=1), \
             or pass --allow-active-gui only if you have stopped nickel yourself"
        );
    }

    // Nickel may still have the touchscreen open while its renderer is
    // paused. Own the selected evdev node exclusively so one tap cannot be
    // delivered to both runtimes. Device::drop explicitly releases the grab.
    input
        .grab_selected()
        .context("claiming the Kobo touchscreen")?;

    let mut fbink = FbInk::new(&args.fbink)?;
    log::info!("kobo refresh helper: {}", fbink.path().display());
    let mut refresh = RefreshPolicy::new(
        info.width,
        info.height,
        args.present_hz,
        args.motion_waveform,
        args.ghost_budget,
    )?;
    let mut runtime = AppRuntime::load(&args, &geometry)?;

    let reload = Arc::new(AtomicBool::new(false));
    let terminate = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(SIGHUP, reload.clone()).context("registering SIGHUP")?;
    signal_hook::flag::register(SIGINT, terminate.clone()).context("registering SIGINT")?;
    signal_hook::flag::register(SIGTERM, terminate.clone()).context("registering SIGTERM")?;

    match args.power_helper.as_deref() {
        Some(helper) if helper.exists() => {
            log::info!("kobo power: short press sleeps via {}", helper.display())
        }
        Some(helper) => log::warn!(
            "kobo power: {} is missing; a short press will do nothing",
            helper.display()
        ),
        None => log::info!("kobo power: key ignored (--no-power-key)"),
    }
    log::info!(
        "kobo runtime ready: logic={}Hz, present={}Hz, motion={:?}, pid={}",
        args.sim_hz,
        args.present_hz,
        args.motion_waveform,
        std::process::id()
    );
    if let Some(root) = std::env::var_os("POCKETJS_DBG_DIR") {
        log::info!(
            "kobo DevTools mailbox root: {}",
            PathBuf::from(root).display()
        );
    }

    // Opt-in, because a report every few seconds is noise in a normal log.
    // The two clock reads per tick cost nothing and are always taken.
    let profile_every = env_parse::<u64>("POCKETJS_PROFILE_SECS")?
        .filter(|secs| *secs > 0)
        .map(Duration::from_secs);
    let mut profile_window = Instant::now();

    let logic_tick = Duration::from_nanos(1_000_000_000 / u64::from(args.sim_hz));
    log::info!(
        "kobo simulation: {}Hz virtual ({} core tick(s) per frame)",
        args.sim_hz,
        CORE_TICKS_PER_SECOND / args.sim_hz
    );

    let started = Instant::now();
    let mut next_tick = Instant::now();
    let mut pending = Vec::<Rect>::new();
    let mut first_frame = true;
    let mut force_refresh = false;
    let mut power_pressed_at: Option<Instant> = None;

    while !terminate.load(Ordering::Relaxed) {
        if reload.swap(false, Ordering::AcqRel) {
            // This point is between guest turns. Keep the old realm alive if
            // a deploy is incomplete or the new bundle throws during boot.
            match AppRuntime::load(&args, &geometry) {
                Ok(next) => {
                    runtime = next;
                    pending.clear();
                    first_frame = true;
                    force_refresh = true;
                    log::info!("kobo reload: new guest installed at frame boundary");
                }
                Err(error) => {
                    log::error!("kobo reload rejected; keeping previous guest: {error:#}");
                }
            }
        }

        for edge in input.poll_power()? {
            match edge {
                PowerKey::Pressed => power_pressed_at = Some(Instant::now()),
                PowerKey::Released => {
                    let held = power_pressed_at.take().map(|at| at.elapsed());
                    let Some(held) = held else { continue };
                    if held >= POWER_LONG_PRESS {
                        log::info!("kobo power: held {held:?}; handing the device back");
                        terminate.store(true, Ordering::Release);
                    } else if let Some(helper) = args.power_helper.as_deref() {
                        if let Err(error) = suspend_through(helper) {
                            log::error!("kobo power: {error:#}");
                        }
                        // Virtual time is a frame counter, so it did not
                        // advance while the machine was down. Reloading
                        // republishes the boot clock, which is the only way a
                        // calendar app comes back showing the right hour.
                        reload.store(true, Ordering::Release);
                        next_tick = Instant::now();
                    }
                }
            }
        }

        let mut catchup = 0;
        while Instant::now() >= next_tick && catchup < MAX_CATCHUP_TICKS {
            let touches = input.poll_touches(&geometry)?;
            // Diff against the last frame actually copied to /dev/fb0, not
            // against the preceding 60Hz simulation frame. This bounds damage
            // while FBInk is busy and lets A -> B -> A disappear before a
            // slower physical present.
            pending = runtime.tick(&touches)?;
            next_tick += logic_tick;
            catchup += 1;
        }
        if catchup > 0 && first_frame {
            pending = vec![Rect {
                x: 0,
                y: 0,
                w: geometry.render_w,
                h: geometry.render_h,
            }];
            first_frame = false;
        }
        if catchup == MAX_CATCHUP_TICKS && Instant::now() >= next_tick {
            log::warn!("kobo runtime missed >{MAX_CATCHUP_TICKS} logic ticks; dropping catch-up");
            next_tick = Instant::now() + logic_tick;
        }

        if profile_every.is_some_and(|interval| profile_window.elapsed() >= interval) {
            let window = profile_window.elapsed();
            log::info!("kobo profile: {}", runtime.profile.drain(window));
            profile_window = Instant::now();
        }

        let presenting = Instant::now();
        if fbink.ready()? {
            let elapsed = started.elapsed();
            if !pending.is_empty() {
                let panel_damage = pending
                    .iter()
                    .copied()
                    .map(|rect| geometry.render_rect_to_panel(rect))
                    .collect::<Vec<_>>();
                if let Some(request) = refresh.on_damage(elapsed, &panel_damage, force_refresh) {
                    framebuffer.write_rects(
                        runtime.damage.current(),
                        geometry.render_w,
                        &pending,
                        &geometry,
                    )?;
                    runtime.damage.latch();
                    pending.clear();
                    force_refresh = false;
                    fbink.submit(request)?;
                }
            } else if let Some(request) = refresh.on_idle(elapsed) {
                fbink.submit(request)?;
            }
        }
        runtime.profile.present += presenting.elapsed();

        let now = Instant::now();
        if next_tick > now {
            std::thread::sleep(next_tick - now);
        }
    }

    fbink
        .finish()
        .context("finishing the final Kobo display refresh")?;
    log::info!("kobo runtime exiting cleanly");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_sim_hz;

    #[test]
    fn the_simulation_rate_must_divide_the_core_tick_rate() {
        for hz in [1, 2, 3, 4, 5, 6, 10, 12, 15, 20, 30, 60] {
            assert_eq!(parse_sim_hz(&hz.to_string()).unwrap(), hz);
        }
        // A rate that does not divide 60 would leave a fractional number of
        // core ticks per frame, which the realm cannot advance.
        for hz in ["0", "7", "45", "61"] {
            let error = parse_sim_hz(hz).unwrap_err().to_string();
            assert!(error.contains("must divide 60 exactly"), "{hz}: {error}");
        }
        assert!(parse_sim_hz("nope").is_err());
    }
}
