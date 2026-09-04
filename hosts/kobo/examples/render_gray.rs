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

fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let [js, pak, out] = args.as_slice() else {
        bail!("usage: render_gray <app.js> <app.pak> <out-prefix>");
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
