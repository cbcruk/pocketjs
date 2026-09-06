//! E-ink refresh policy and the panel update ioctl.
//!
//! This used to shell out to an installed FBInk CLI, on the reasoning that
//! FBInk carries Kobo's per-generation mxcfb quirks so the host need not. The
//! cost of that convenience was a dynamic dependency, and it came due: FBInk
//! is a hard-float binary, the Glo's 2012 firmware ships a soft-float
//! userspace, and the host — static musl, indifferent to any of that — could
//! not start it. One dependency tied a runtime that needs nothing to a
//! particular firmware.
//!
//! So the update goes through the ioctl directly. There is exactly one device
//! to support, and its interface is fixed: a Mark 4 i.MX50 running the NTX
//! 2.6.35 kernel, which takes `mxcfb_update_data_v1_ntx` on
//! `MXCFB_SEND_UPDATE`. The constants are transcribed from FBInk's
//! `eink/mxcfb-kobo.h`, which remains the reference for what these numbers
//! mean; what is gone is the requirement that FBInk be installed and runnable.

use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::damage::{Rect, merge_all};

const MOTION_WINDOW: Duration = Duration::from_millis(120);
const CLEANUP_QUIET: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    Auto,
    Du,
    A2,
    Gc16,
}

impl Waveform {
    /// NTX waveform ids. `AUTO` is not one of them — it is the driver's own
    /// sentinel telling the EPDC to pick, so it sits outside the 0..11 range.
    fn as_ntx(self) -> u32 {
        match self {
            Self::Auto => WAVEFORM_MODE_AUTO,
            Self::Du => NTX_WFM_MODE_DU,
            Self::A2 => NTX_WFM_MODE_A2,
            Self::Gc16 => NTX_WFM_MODE_GC16,
        }
    }

    pub fn parse_motion(value: &str) -> Result<Self> {
        match value.to_ascii_uppercase().as_str() {
            "DU" => Ok(Self::Du),
            "A2" => Ok(Self::A2),
            _ => bail!("motion waveform must be DU or A2 (got {value:?})"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshKind {
    Initial,
    Motion,
    QuietCleanup,
    GhostCleanup,
    Forced,
    Static,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RefreshRequest {
    pub rect: Rect,
    pub waveform: Waveform,
    pub flash: bool,
    pub kind: RefreshKind,
}

pub struct RefreshPolicy {
    panel: Rect,
    motion_waveform: Waveform,
    min_present_interval: Duration,
    ghost_update_limit: u32,
    ghost_area_limit: usize,
    started: bool,
    last_damage: Option<Duration>,
    last_present: Option<Duration>,
    cleanup_at: Option<Duration>,
    cleanup_region: Option<Rect>,
    fast_updates: u32,
    fast_area: usize,
}

impl RefreshPolicy {
    pub fn new(
        panel_width: usize,
        panel_height: usize,
        present_hz: u32,
        motion_waveform: Waveform,
        ghost_update_limit: u32,
    ) -> Result<Self> {
        if present_hz == 0 || present_hz > 60 {
            bail!("present rate must be in 1..=60 Hz");
        }
        let panel = Rect {
            x: 0,
            y: 0,
            w: panel_width,
            h: panel_height,
        };
        Ok(Self {
            panel,
            motion_waveform,
            min_present_interval: Duration::from_nanos(1_000_000_000 / present_hz as u64),
            ghost_update_limit: ghost_update_limit.max(1),
            ghost_area_limit: panel_width * panel_height * 6,
            started: false,
            last_damage: None,
            last_present: None,
            cleanup_at: None,
            cleanup_region: None,
            fast_updates: 0,
            fast_area: 0,
        })
    }

    /// Decide whether the latest framebuffer damage should be submitted now.
    /// The caller retains damage while this returns `None`, keeping simulation
    /// at 60Hz independently of the panel present rate.
    pub fn on_damage(
        &mut self,
        now: Duration,
        damage: &[Rect],
        force_full: bool,
    ) -> Option<RefreshRequest> {
        if force_full {
            return Some(self.full(RefreshKind::Forced));
        }
        let damage = merge_all(damage)?;

        if !self.started {
            self.started = true;
            self.last_damage = Some(now);
            self.last_present = Some(now);
            return Some(RefreshRequest {
                rect: damage,
                waveform: Waveform::Auto,
                flash: false,
                kind: RefreshKind::Initial,
            });
        }

        if self.fast_updates >= self.ghost_update_limit || self.fast_area >= self.ghost_area_limit {
            return Some(self.full(RefreshKind::GhostCleanup));
        }

        let moving = self
            .last_damage
            .is_some_and(|last| now.saturating_sub(last) <= MOTION_WINDOW);
        self.last_damage = Some(now);

        if moving {
            self.cleanup_region = Some(match self.cleanup_region {
                Some(previous) => previous.union(damage),
                None => damage,
            });
            self.cleanup_at = Some(now + CLEANUP_QUIET);
            if self
                .last_present
                .is_some_and(|last| now.saturating_sub(last) < self.min_present_interval)
            {
                return None;
            }
            self.last_present = Some(now);
            self.fast_updates += 1;
            self.fast_area = self.fast_area.saturating_add(damage.w * damage.h);
            Some(RefreshRequest {
                rect: damage,
                waveform: self.motion_waveform,
                flash: false,
                kind: RefreshKind::Motion,
            })
        } else {
            self.last_present = Some(now);
            Some(RefreshRequest {
                rect: damage,
                waveform: Waveform::Auto,
                flash: false,
                kind: RefreshKind::Static,
            })
        }
    }

    /// Emit the quiet high-quality pass after motion, even with no new pixels.
    pub fn on_idle(&mut self, now: Duration) -> Option<RefreshRequest> {
        let due = self.cleanup_at.is_some_and(|at| now >= at);
        if !due {
            return None;
        }
        self.cleanup_at = None;
        let rect = self.cleanup_region.take()?;
        self.last_present = Some(now);
        self.fast_updates = 0;
        self.fast_area = 0;
        Some(RefreshRequest {
            rect,
            waveform: Waveform::Gc16,
            flash: true,
            kind: RefreshKind::QuietCleanup,
        })
    }

    fn full(&mut self, kind: RefreshKind) -> RefreshRequest {
        self.started = true;
        self.last_damage = None;
        self.last_present = None;
        self.cleanup_at = None;
        self.cleanup_region = None;
        self.fast_updates = 0;
        self.fast_area = 0;
        RefreshRequest {
            rect: self.panel,
            waveform: Waveform::Gc16,
            flash: true,
            kind,
        }
    }
}

// Transcribed from FBInk's eink/mxcfb-kobo.h. The Glo is a Mark 4 i.MX50 on
// the NTX 2.6.35 kernel, which is the `_v1_ntx` shape of the interface.
const NTX_WFM_MODE_DU: u32 = 1;
const NTX_WFM_MODE_GC16: u32 = 2;
const NTX_WFM_MODE_A2: u32 = 4;
/// Not an NTX mode id: the driver's "you choose" sentinel.
const WAVEFORM_MODE_AUTO: u32 = 257;
const UPDATE_MODE_PARTIAL: u32 = 0;
const UPDATE_MODE_FULL: u32 = 1;
const TEMP_USE_AMBIENT: i32 = 0x1000;
/// `_IOW('F', 0x2E, struct mxcfb_update_data_v1_ntx)`, the struct being 68 bytes.
#[cfg(target_os = "linux")]
const MXCFB_SEND_UPDATE_V1_NTX: libc::c_ulong = 0x4044_462E;
/// `_IOW('F', 0x2F, uint32_t)` — the mx50/NTX flavour, which takes the marker
/// by value rather than the later struct.
#[cfg(target_os = "linux")]
const MXCFB_WAIT_FOR_UPDATE_COMPLETE_V1: libc::c_ulong = 0x4004_462F;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct MxcfbRect {
    top: u32,
    left: u32,
    width: u32,
    height: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MxcfbAltBufferDataNtx {
    /// A kernel-side pointer this host never populates. Declared as the 32-bit
    /// word it is on the target rather than as a pointer, so the struct keeps
    /// the same 68-byte shape when built for a 64-bit host.
    virt_addr: u32,
    phys_addr: u32,
    width: u32,
    height: u32,
    alt_update_region: MxcfbRect,
}

/// The ioctl number encodes this size, so a layout that drifts is not a subtle
/// bug — the driver rejects it, or worse, reads the wrong fields.
const _: () = assert!(std::mem::size_of::<MxcfbUpdateDataV1Ntx>() == 68);

#[repr(C)]
#[derive(Clone, Copy)]
struct MxcfbUpdateDataV1Ntx {
    update_region: MxcfbRect,
    waveform_mode: u32,
    update_mode: u32,
    update_marker: u32,
    temp: i32,
    flags: u32,
    alt_buffer_data: MxcfbAltBufferDataNtx,
}

/// The panel's update queue, addressed straight through the framebuffer.
pub struct Epdc {
    path: PathBuf,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    file: File,
    marker: u32,
    pending: Option<u32>,
}

impl Epdc {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("opening {} for panel updates", path.display()))?;
        Ok(Self {
            path,
            file,
            marker: 0,
            pending: None,
        })
    }

    /// Always ready. Backpressure used to come from reaping a child process;
    /// now it comes from RefreshPolicy, which already paces to `present_hz`
    /// and holds motion updates in a window. The EPDC queues what it is given.
    pub fn ready(&mut self) -> Result<bool> {
        Ok(true)
    }

    pub fn submit(&mut self, request: RefreshRequest) -> Result<()> {
        let Rect { x, y, w, h } = request.rect;
        self.marker = self.marker.wrapping_add(1).max(1);
        let update = MxcfbUpdateDataV1Ntx {
            update_region: MxcfbRect {
                top: y as u32,
                left: x as u32,
                width: w as u32,
                height: h as u32,
            },
            waveform_mode: request.waveform.as_ntx(),
            // FULL is the flashing update; PARTIAL leaves the rest alone.
            update_mode: if request.flash {
                UPDATE_MODE_FULL
            } else {
                UPDATE_MODE_PARTIAL
            },
            update_marker: self.marker,
            temp: TEMP_USE_AMBIENT,
            flags: 0,
            alt_buffer_data: MxcfbAltBufferDataNtx {
                virt_addr: 0,
                phys_addr: 0,
                width: 0,
                height: 0,
                alt_update_region: MxcfbRect::default(),
            },
        };
        log::debug!(
            "kobo refresh: {:?} {:?} {}x{}+{},{} marker={}",
            request.kind,
            request.waveform,
            w,
            h,
            x,
            y,
            self.marker
        );
        self.send(&update)?;
        self.pending = Some(self.marker);
        Ok(())
    }

    /// Wait for the last update to reach the glass before the caller hands the
    /// panel back. Without it the UI returns mid-refresh and the final frame
    /// is whatever the EPDC happened to have finished.
    pub fn finish(&mut self) -> Result<()> {
        let Some(marker) = self.pending.take() else {
            return Ok(());
        };
        self.wait_for(marker)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(target_os = "linux")]
impl Epdc {
    fn send(&mut self, update: &MxcfbUpdateDataV1Ntx) -> Result<()> {
        use std::os::fd::AsRawFd;

        // SAFETY: the request encodes the struct the driver expects, and the
        // pointer is a live borrow of exactly that struct.
        let result = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                MXCFB_SEND_UPDATE_V1_NTX as _,
                update as *const MxcfbUpdateDataV1Ntx,
            )
        };
        if result < 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("MXCFB_SEND_UPDATE on {}", self.path.display()));
        }
        Ok(())
    }

    fn wait_for(&mut self, marker: u32) -> Result<()> {
        use std::os::fd::AsRawFd;

        // SAFETY: the mx50 flavour takes the marker by value, not by pointer.
        let result = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                MXCFB_WAIT_FOR_UPDATE_COMPLETE_V1 as _,
                marker as libc::c_ulong,
            )
        };
        if result < 0 {
            // A marker the driver has already retired is not an error worth
            // failing a shutdown over.
            log::debug!(
                "kobo refresh: waiting for marker {marker} returned {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
impl Epdc {
    fn send(&mut self, _update: &MxcfbUpdateDataV1Ntx) -> Result<()> {
        bail!("panel updates are only supported on Linux")
    }

    fn wait_for(&mut self, _marker: u32) -> Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: usize, y: usize, w: usize, h: usize) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn first_update_is_conservative_auto() {
        let mut policy = RefreshPolicy::new(100, 200, 30, Waveform::Du, 8).unwrap();
        let request = policy
            .on_damage(Duration::ZERO, &[rect(1, 2, 3, 4)], false)
            .unwrap();
        assert_eq!(request.waveform, Waveform::Auto);
        assert_eq!(request.kind, RefreshKind::Initial);
        assert!(!request.flash);
    }

    #[test]
    fn motion_is_throttled_then_gets_quiet_cleanup() {
        let mut policy = RefreshPolicy::new(100, 200, 30, Waveform::A2, 8).unwrap();
        policy.on_damage(Duration::ZERO, &[rect(0, 0, 5, 5)], false);
        assert!(
            policy
                .on_damage(Duration::from_millis(10), &[rect(5, 0, 5, 5)], false)
                .is_none()
        );
        let motion = policy
            .on_damage(Duration::from_millis(40), &[rect(5, 0, 5, 5)], false)
            .unwrap();
        assert_eq!(motion.waveform, Waveform::A2);
        assert_eq!(motion.kind, RefreshKind::Motion);
        assert!(!motion.flash);
        assert!(policy.on_idle(Duration::from_millis(239)).is_none());
        let cleanup = policy.on_idle(Duration::from_millis(240)).unwrap();
        assert_eq!(cleanup.kind, RefreshKind::QuietCleanup);
        assert_eq!(cleanup.waveform, Waveform::Gc16);
        assert!(cleanup.flash);
        assert_eq!(cleanup.rect, rect(5, 0, 5, 5));
    }

    #[test]
    fn ghost_budget_forces_full_flash() {
        let mut policy = RefreshPolicy::new(100, 200, 60, Waveform::Du, 1).unwrap();
        policy.on_damage(Duration::ZERO, &[rect(0, 0, 5, 5)], false);
        let fast = policy
            .on_damage(Duration::from_millis(20), &[rect(0, 0, 5, 5)], false)
            .unwrap();
        assert_eq!(fast.kind, RefreshKind::Motion);
        let full = policy
            .on_damage(Duration::from_millis(40), &[rect(0, 0, 5, 5)], false)
            .unwrap();
        assert_eq!(full.kind, RefreshKind::GhostCleanup);
        assert_eq!(full.rect, rect(0, 0, 100, 200));
        assert_eq!(full.waveform, Waveform::Gc16);
        assert!(full.flash);
    }

    #[test]
    fn explicit_reload_forces_full_cleanup() {
        let mut policy = RefreshPolicy::new(100, 200, 30, Waveform::Du, 8).unwrap();
        let full = policy
            .on_damage(Duration::ZERO, &[rect(1, 2, 3, 4)], true)
            .unwrap();
        assert_eq!(full.kind, RefreshKind::Forced);
        assert_eq!(full.waveform, Waveform::Gc16);
        assert!(full.flash);
    }

    #[cfg(unix)]
    #[test]
    fn the_ioctl_numbers_match_their_iow_encoding() {
        // Hand-transcribed hex is exactly the kind of constant that fails
        // silently: a wrong number is a panel that never updates.
        fn iow(kind: u8, nr: u8, size: usize) -> u64 {
            (1 << 30) | ((size as u64) << 16) | ((kind as u64) << 8) | nr as u64
        }
        assert_eq!(
            iow(b'F', 0x2E, std::mem::size_of::<MxcfbUpdateDataV1Ntx>()),
            0x4044_462E
        );
        assert_eq!(iow(b'F', 0x2F, std::mem::size_of::<u32>()), 0x4004_462F);
    }

    #[test]
    fn waveforms_map_to_ntx_ids_and_auto_stays_the_sentinel() {
        assert_eq!(Waveform::Du.as_ntx(), 1);
        assert_eq!(Waveform::Gc16.as_ntx(), 2);
        assert_eq!(Waveform::A2.as_ntx(), 4);
        // AUTO is the driver telling itself to choose, not an NTX mode id, so
        // it must stay outside the 0..11 range the others live in.
        assert_eq!(Waveform::Auto.as_ntx(), 257);
    }
}
