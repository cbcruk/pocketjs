//! Headless Gray8 snapshot of a guest bundle at the kobo-glo contract.
//!
//! The device host writes `/dev/fb0` and can only run on a Kobo, so this
//! example is how the Gray8 pipeline (pak -> guest frame -> DrawList ->
//! `render_scaled_gray8`) is inspected before any hardware is involved. It
//! writes binary PGM (P5), the grayscale sibling of the PPM snapshots
//! engine/ios/examples/render_hero.rs produces.
//!
//!   bun tools/pocket.ts compile --target kobo-glo \
//!     --manifest apps/paper-ink/pocket.json --project-root .
//!   cargo run --manifest-path hosts/kobo/Cargo.toml --example render_gray -- \
//!     dist/paper-ink-main.js dist/paper-ink-main.pak /tmp/paper-ink
//!
//! Exit is nonzero if the rendered frame is blank, which is the failure this
//! check exists to catch: a bundle that boots but paints nothing looks
//! identical to a working one in the host's logs.
//!
//! `--touch X,Y` drags a synthetic contact across the surface before the
//! snapshot. An idle first frame exercises far less of an app than a touch
//! does — paper-ink only builds its ink nodes, and only touches the style
//! props they carry, once a contact exists — so without this a bundle can
//! pass here and still throw on the device the moment a finger lands.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use pocket_mod::Guest;
use pocket_ui_surface::UiSurface;
use pocketjs_core::{raster, spec};

const HOST_ID: &str = "kobo-glo";
const HOST_ABI: u32 = 5;
const LOGICAL_W: usize = 379;
const LOGICAL_H: usize = 512;
const DENSITY: usize = 2;
/// Enough ticks for mount effects and any entry transition to settle.
const FRAMES: u32 = 60;
/// Frames the synthetic contact is held for. It moves every frame because an
/// app may ignore a contact that has not changed position.
const TOUCH_FRAMES: u32 = 8;

/// framework/src/touch.ts legacy form: `(id:8 << 18) | (y:9 << 9) | x:9`.
fn pack_touch(id: u32, x: u32, y: u32) -> u32 {
    ((id & 0xff) << 18) | ((y & 0x1ff) << 9) | (x & 0x1ff)
}

fn parse_touch(value: &str) -> Result<(u32, u32)> {
    let Some((x, y)) = value.split_once(',') else {
        bail!("--touch wants X,Y in logical coordinates (got {value:?})");
    };
    let x: u32 = x.trim().parse().context("--touch X")?;
    let y: u32 = y.trim().parse().context("--touch Y")?;
    if x as usize >= LOGICAL_W || y as usize >= LOGICAL_H {
        bail!("--touch {x},{y} is outside the {LOGICAL_W}x{LOGICAL_H} logical viewport");
    }
    Ok((x, y))
}

fn main() -> Result<()> {
    let words = std::env::args().skip(1).collect::<Vec<_>>();
    let mut positional = Vec::new();
    let mut touch = None;
    let mut index = 0;
    while index < words.len() {
        if words[index] == "--touch" {
            let value = words
                .get(index + 1)
                .ok_or_else(|| anyhow::anyhow!("--touch requires a value"))?;
            touch = Some(parse_touch(value)?);
            index += 2;
            continue;
        }
        positional.push(words[index].clone());
        index += 1;
    }
    let [js, pak, out] = positional.as_slice() else {
        bail!("usage: render_gray <app.js> <app.pak> <out-prefix> [--touch X,Y]");
    };
    let out = PathBuf::from(out);

    let bundle = std::fs::read_to_string(js).with_context(|| format!("reading bundle {js}"))?;
    let pak_bytes = std::fs::read(pak).with_context(|| format!("reading pak {pak}"))?;

    let surface = UiSurface::new_with_density(
        (LOGICAL_W as f32, LOGICAL_H as f32),
        DENSITY as u32,
    );
    surface.set_identity(HOST_ID, HOST_ABI);
    surface.feed_pak(&pak_bytes);
    let guest = Guest::new().context("creating PocketJS guest")?;
    surface.mount(&guest).context("mounting UI surface")?;
    // The device host publishes the real local time here. A snapshot wants a
    // fixed one instead, so the same bundle always produces the same PGM.
    guest
        .eval(
            "boot-clock",
            "globalThis.__bootClock = { year: 2026, month: 9, day: 5, \
             weekday: 5, secondOfDay: 35100 };",
        )
        .context("publishing the snapshot boot clock")?;
    guest.eval("app", &bundle).context("evaluating app bundle")?;
    if !guest.has_frame() {
        bail!("{js} installed no global frame(); was it built for {HOST_ID}?");
    }

    let render_w = LOGICAL_W * DENSITY;
    let render_h = LOGICAL_H * DENSITY;
    let mut fb = vec![0u8; render_w * render_h];

    for tick in 0..FRAMES {
        guest
            .frame_with_touches(0, spec::ANALOG_CENTER, &[])
            .with_context(|| format!("guest frame {tick}"))?;
        surface.tick();
    }
    if let Some((x, y)) = touch {
        for step in 0..TOUCH_FRAMES {
            let x = (x + step).min(LOGICAL_W as u32 - 1);
            let y = (y + step).min(LOGICAL_H as u32 - 1);
            guest
                .frame_with_touches(0, spec::ANALOG_CENTER, &[pack_touch(0, x, y)])
                .with_context(|| format!("guest frame with contact at {x},{y}"))?;
            surface.tick();
        }
        // Release, so a teardown path runs too.
        guest
            .frame_with_touches(0, spec::ANALOG_CENTER, &[])
            .context("guest frame after the contact lifted")?;
        surface.tick();
    }
    surface.with_ui(|ui| {
        let words = ui.draw().words.clone();
        raster::render_scaled_gray8(ui, &words, &mut fb, DENSITY as u32);
    });

    let first = fb[0];
    if fb.iter().all(|pixel| *pixel == first) {
        bail!(
            "rendered {render_w}x{render_h} frame is uniform (every pixel {first}); \
             the bundle booted but painted nothing"
        );
    }

    let path = out.with_extension("pgm");
    let mut pgm = format!("P5\n{render_w} {render_h}\n255\n").into_bytes();
    pgm.extend_from_slice(&fb);
    std::fs::write(&path, pgm).with_context(|| format!("writing {}", path.display()))?;

    let ink = fb.iter().filter(|pixel| **pixel < 128).count();
    println!(
        "{} — {render_w}x{render_h} Gray8, {ink} dark px ({:.1}%), {} distinct levels",
        path.display(),
        ink as f64 * 100.0 / fb.len() as f64,
        {
            let mut seen = [false; 256];
            for pixel in &fb {
                seen[*pixel as usize] = true;
            }
            seen.iter().filter(|s| **s).count()
        }
    );
    Ok(())
}
