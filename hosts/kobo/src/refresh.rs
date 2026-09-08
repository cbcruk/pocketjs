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
// Timing a wait needs a clock, and waiting is a Linux ioctl; on other targets
// the probe does not compile at all.
#[cfg(target_os = "linux")]
use std::time::Instant;

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

    /// Dense index, for keeping one measurement per waveform.
    pub fn index(self) -> usize {
        match self {
            Self::Auto => 0,
            Self::Du => 1,
            Self::A2 => 2,
            Self::Gc16 => 3,
        }
    }

    /// Number of distinct waveforms, for sizing that table.
    pub const COUNT: usize = 4;

    /// The name this waveform is selected by, so a bundle can say on screen
    /// which run it is looking at. Tuning ghosting means comparing two runs
    /// by eye, and an unlabelled screenshot is worth nothing an hour later.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "AUTO",
            Self::Du => "DU",
            Self::A2 => "A2",
            Self::Gc16 => "GC16",
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
    /// Fast updates allowed before a full cleanup.
    ghost_update_limit: u32,
    /// Fast-updated pixels allowed before a full cleanup. Whichever limit is
    /// reached first wins, and for anything but a tiny damage rect that is
    /// this one: a probe repainting a third of the panel exhausts six panels
    /// of area in about twelve updates, so a `--ghost-budget` of 80 produced
    /// a cleanup every 1.25s and never once ran out of updates.
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
        ghost_area_panels: usize,
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
            ghost_area_limit: panel_width * panel_height * ghost_area_panels.max(1),
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
/// The same call declared the other direction, which some NTX kernels ship.
/// Only `--probe-epdc` uses it: this kernel answers ENOTTY, but a different
/// device is why the probe tries more than one.
#[cfg(target_os = "linux")]
const MXCFB_WAIT_FOR_UPDATE_COMPLETE_R: libc::c_ulong = 0x8004_462F;
/// The v2 form: a struct holding the marker and a collision-test rect.
#[cfg(target_os = "linux")]
const MXCFB_WAIT_FOR_UPDATE_COMPLETE_V2: libc::c_ulong = 0xC008_4635;

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
    failed_waits: u64,
    last_wait_errno: i32,
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
            failed_waits: 0,
            last_wait_errno: 0,
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

    /// Waits that the driver refused, since the session started.
    ///
    /// A wait that fails is indistinguishable from a panel that was already
    /// finished — both return at once — so without this the difference
    /// between "the update settled" and "the ioctl was rejected" is invisible.
    pub fn failed_waits(&self) -> u64 {
        self.failed_waits
    }

    /// errno from the most recent refused wait, or 0 if none has been refused.
    /// EFAULT means the driver wanted a pointer where we passed a value.
    pub fn last_wait_errno(&self) -> i32 {
        self.last_wait_errno
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

        // The marker goes by pointer. Passing it by value is accepted by the
        // compiler and rejected by the kernel with EINVAL, which is
        // indistinguishable from a panel that had nothing left to do: both
        // return instantly. This host ran that way for its whole life, with no
        // back-pressure at all, asking a panel that needs 1030ms per full
        // flash for one every 1.25s — until the EPDC stopped accepting
        // updates. `--probe-epdc` is what settled it.
        let mut marker = marker;
        // SAFETY: the pointer is a live borrow of a u32 the ioctl reads.
        let result = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                MXCFB_WAIT_FOR_UPDATE_COMPLETE_V1 as _,
                &mut marker as *mut u32,
            )
        };
        if result < 0 {
            // A marker the driver has already retired is not an error worth
            // failing a shutdown over.
            self.failed_waits += 1;
            self.last_wait_errno = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
            log::debug!(
                "kobo refresh: waiting for marker {marker} returned {}",
                std::io::Error::last_os_error()
            );
        }
        Ok(())
    }
}

/// One way of asking the driver to wait, and what it answered.
pub struct WaitProbe {
    pub name: &'static str,
    pub request: libc::c_ulong,
    pub errno: i32,
    pub elapsed: Duration,
}

#[cfg(target_os = "linux")]
impl Epdc {
    /// Try each known encoding of MXCFB_WAIT_FOR_UPDATE_COMPLETE against a
    /// real in-flight update, and report what each one said.
    ///
    /// The encoding differs between i.MX kernels and the NTX forks, and being
    /// wrong is silent: a wait that is rejected returns as fast as one with
    /// nothing to wait for, so the host runs with no back-pressure at all and
    /// only finds out when the EPDC stops accepting updates. This device
    /// answered EINVAL to the mainline encoding for every update of every
    /// session before anyone counted.
    pub fn probe_waits(&mut self, panel_w: usize, panel_h: usize) -> Result<Vec<WaitProbe>> {
        use std::os::fd::AsRawFd;

        let mut out = Vec::new();
        let fd = self.file.as_raw_fd();
        for (name, request) in [
            ("_IOW('F',0x2F,u32) by value", MXCFB_WAIT_FOR_UPDATE_COMPLETE_V1),
            ("_IOW('F',0x2F,u32) by pointer", MXCFB_WAIT_FOR_UPDATE_COMPLETE_V1),
            ("_IOR('F',0x2F,u32) by pointer", MXCFB_WAIT_FOR_UPDATE_COMPLETE_R),
            ("_IOWR('F',0x35,marker_data)", MXCFB_WAIT_FOR_UPDATE_COMPLETE_V2),
        ] {
            // A fresh full-panel update, so there is genuinely something in
            // flight: waiting on a marker the driver already retired proves
            // nothing about whether the wait works.
            self.submit(RefreshRequest {
                rect: Rect {
                    x: 0,
                    y: 0,
                    w: panel_w,
                    h: panel_h,
                },
                waveform: Waveform::Gc16,
                flash: true,
                kind: RefreshKind::Forced,
            })?;
            let marker = self.pending.take().unwrap_or(self.marker);
            let by_pointer = name.contains("pointer");
            let v2 = name.contains("0x35");
            let started = Instant::now();
            // SAFETY: each arm passes exactly what its comment describes, and
            // both buffers outlive the call.
            let result = unsafe {
                if v2 {
                    let mut data = [marker, 0u32];
                    libc::ioctl(fd, request as _, data.as_mut_ptr())
                } else if by_pointer {
                    let mut value = marker;
                    libc::ioctl(fd, request as _, &mut value as *mut u32)
                } else {
                    libc::ioctl(fd, request as _, marker as libc::c_ulong)
                }
            };
            out.push(WaitProbe {
                name,
                request,
                errno: if result < 0 {
                    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
                } else {
                    0
                },
                elapsed: started.elapsed(),
            });
        }
        Ok(out)
    }
}

#[cfg(not(target_os = "linux"))]
impl Epdc {
    pub fn probe_waits(&mut self, _w: usize, _h: usize) -> Result<Vec<WaitProbe>> {
        bail!("panel probing is only supported on Linux")
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
        let mut policy = RefreshPolicy::new(100, 200, 30, Waveform::Du, 8, 6).unwrap();
        let request = policy
            .on_damage(Duration::ZERO, &[rect(1, 2, 3, 4)], false)
            .unwrap();
        assert_eq!(request.waveform, Waveform::Auto);
        assert_eq!(request.kind, RefreshKind::Initial);
        assert!(!request.flash);
    }

    #[test]
    fn motion_is_throttled_then_gets_quiet_cleanup() {
        let mut policy = RefreshPolicy::new(100, 200, 30, Waveform::A2, 8, 6).unwrap();
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
        let mut policy = RefreshPolicy::new(100, 200, 60, Waveform::Du, 1, 6).unwrap();
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

    /// The count is not the limit that fires in practice. A screen repainting
    /// a large rect exhausts the area budget in a handful of updates, so a
    /// generous `--ghost-budget` buys nothing on its own — measured on a Glo
    /// as a full-screen cleanup every 1.25s while `--ghost-budget 80` had
    /// never once run out of updates.
    #[test]
    fn area_reaches_the_cleanup_before_the_count_does() {
        // Two panels of area against a rect that is half a panel. The limit is
        // read before the update is counted, so the fourth is the last one
        // through and the fifth is the cleanup.
        let mut policy = RefreshPolicy::new(100, 200, 60, Waveform::Du, 1000, 2).unwrap();
        let half = rect(0, 0, 100, 100);
        policy.on_damage(Duration::ZERO, &[half], false);
        for tick in 1..=4 {
            let request = policy
                .on_damage(Duration::from_millis(tick * 20), &[half], false)
                .unwrap();
            assert_eq!(request.kind, RefreshKind::Motion, "update {tick}");
        }
        let full = policy
            .on_damage(Duration::from_millis(100), &[half], false)
            .unwrap();
        assert_eq!(full.kind, RefreshKind::GhostCleanup);

        // The same run with room for the area never reaches a cleanup.
        let mut roomy = RefreshPolicy::new(100, 200, 60, Waveform::Du, 1000, 64).unwrap();
        roomy.on_damage(Duration::ZERO, &[half], false);
        for tick in 1..=8 {
            let request = roomy
                .on_damage(Duration::from_millis(tick * 20), &[half], false)
                .unwrap();
            assert_eq!(request.kind, RefreshKind::Motion, "roomy update {tick}");
        }
    }

    #[test]
    fn explicit_reload_forces_full_cleanup() {
        let mut policy = RefreshPolicy::new(100, 200, 30, Waveform::Du, 8, 6).unwrap();
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
