//! Minimal generic Linux evdev touch reader. It deliberately consumes the
//! stable kernel event ABI directly, avoiding assumptions about Kobo event
//! node numbers or a vendor input library. The Glo's infrared (Neonode)
//! digitizer is single-contact and reports ABS_X/ABS_Y + BTN_TOUCH rather
//! than the multitouch protocol, which the non-MT branch below covers.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use std::fs::File;

#[cfg(target_os = "linux")]
use anyhow::Context;
use anyhow::{Result, bail};

use crate::geometry::Geometry;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0;
const SYN_DROPPED: u16 = 3;
const BTN_TOUCH: u16 = 0x14a;
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_MT_SLOT: u16 = 0x2f;
const ABS_MT_POSITION_X: u16 = 0x35;
const ABS_MT_POSITION_Y: u16 = 0x36;
const ABS_MT_TRACKING_ID: u16 = 0x39;
const MAX_CONTACTS: usize = 8;
const TOUCH_ID_MASK: u32 = 0xff;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AxisRange {
    min: i32,
    max: i32,
}

#[derive(Clone, Copy, Debug, Default)]
struct Contact {
    active: bool,
    id: u32,
    x: i32,
    y: i32,
}

#[derive(Clone, Debug)]
struct TouchState {
    contacts: [Contact; MAX_CONTACTS],
    current_slot: Option<usize>,
    mt: bool,
    sync_lost: bool,
}

impl TouchState {
    fn new(mt: bool) -> Self {
        Self {
            contacts: [Contact::default(); MAX_CONTACTS],
            current_slot: Some(0),
            mt,
            sync_lost: false,
        }
    }

    fn apply(&mut self, type_: u16, code: u16, value: i32) -> bool {
        if type_ == EV_SYN && code == SYN_DROPPED {
            self.contacts.fill(Contact::default());
            self.current_slot = if self.mt { None } else { Some(0) };
            self.sync_lost = true;
            return true;
        }
        if self.sync_lost {
            if type_ == EV_SYN && code == SYN_REPORT {
                self.sync_lost = false;
            }
            return false;
        }

        match (type_, code) {
            (EV_ABS, ABS_MT_SLOT) if self.mt => {
                self.current_slot = usize::try_from(value)
                    .ok()
                    .filter(|slot| *slot < MAX_CONTACTS);
            }
            (EV_ABS, ABS_MT_TRACKING_ID) if self.mt => {
                if let Some(contact) = self
                    .current_slot
                    .and_then(|slot| self.contacts.get_mut(slot))
                {
                    contact.active = value >= 0;
                    if value >= 0 {
                        contact.id = value as u32;
                    }
                }
            }
            (EV_ABS, ABS_MT_POSITION_X) if self.mt => {
                if let Some(contact) = self
                    .current_slot
                    .and_then(|slot| self.contacts.get_mut(slot))
                {
                    contact.x = value;
                }
            }
            (EV_ABS, ABS_MT_POSITION_Y) if self.mt => {
                if let Some(contact) = self
                    .current_slot
                    .and_then(|slot| self.contacts.get_mut(slot))
                {
                    contact.y = value;
                }
            }
            (EV_ABS, ABS_X) if !self.mt => self.contacts[0].x = value,
            (EV_ABS, ABS_Y) if !self.mt => self.contacts[0].y = value,
            (EV_KEY, BTN_TOUCH) if !self.mt => {
                self.contacts[0].active = value != 0;
                self.contacts[0].id = 0;
            }
            (EV_SYN, SYN_REPORT) => {}
            _ => {}
        }
        false
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Calibration {
    swap_xy: bool,
    flip_x: bool,
    flip_y: bool,
}

impl Calibration {
    fn from_env() -> Self {
        let enabled = |name: &str| {
            std::env::var(name)
                .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
                .unwrap_or(false)
        };
        Self {
            swap_xy: enabled("POCKETJS_TOUCH_SWAP_XY"),
            flip_x: enabled("POCKETJS_TOUCH_FLIP_X"),
            flip_y: enabled("POCKETJS_TOUCH_FLIP_Y"),
        }
    }
}

/// Replacements for the axis extents a digitizer declares through
/// `EVIOCGABS`.
///
/// A driver is free to declare a coordinate space it does not actually use,
/// and the Glo's zForce does exactly that: it advertises 0..1200 by 0..1600
/// while reporting panel pixels, so normalizing against the declaration
/// confines every touch to roughly the top-left half of the screen. Measure
/// with `--probe-touch` and override; nothing here guesses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RangeOverride {
    x_min: Option<i32>,
    x_max: Option<i32>,
    y_min: Option<i32>,
    y_max: Option<i32>,
}

impl RangeOverride {
    fn from_env() -> Result<Self> {
        let bound = |name: &str| -> Result<Option<i32>> {
            std::env::var(name)
                .ok()
                .map(|value| {
                    value
                        .parse::<i32>()
                        .map_err(|error| anyhow::anyhow!("{name}={value:?}: {error}"))
                })
                .transpose()
        };
        Ok(Self {
            x_min: bound("POCKETJS_TOUCH_X_MIN")?,
            x_max: bound("POCKETJS_TOUCH_X_MAX")?,
            y_min: bound("POCKETJS_TOUCH_Y_MIN")?,
            y_max: bound("POCKETJS_TOUCH_Y_MAX")?,
        })
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Apply the overrides to a probed pair of ranges, rejecting a result that
    /// would make `normalize` degenerate.
    fn apply(&self, x: AxisRange, y: AxisRange) -> Result<(AxisRange, AxisRange)> {
        let merged = |range: AxisRange, min: Option<i32>, max: Option<i32>, axis: char| {
            let range = AxisRange {
                min: min.unwrap_or(range.min),
                max: max.unwrap_or(range.max),
            };
            if range.max <= range.min {
                bail!(
                    "touch {axis} range override is empty: min {} is not below max {}",
                    range.min,
                    range.max
                );
            }
            Ok(range)
        };
        Ok((
            merged(x, self.x_min, self.x_max, 'x')?,
            merged(y, self.y_min, self.y_max, 'y')?,
        ))
    }
}

/// One active contact in every coordinate space the host derives from it.
///
/// The runtime only ever needs the logical pair, but a device bring-up session
/// cannot tell a calibration error from a geometry error without seeing the
/// raw kernel values that produced it, so `--probe-touch` reports all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactReport {
    pub slot: usize,
    pub raw_x: i32,
    pub raw_y: i32,
    pub panel_x: usize,
    pub panel_y: usize,
    pub logical_x: u32,
    pub logical_y: u32,
}

/// The evdev axis contract of the touchscreen the host selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TouchAxes {
    pub path: String,
    pub multitouch: bool,
    pub x_min: i32,
    pub x_max: i32,
    pub y_min: i32,
    pub y_max: i32,
}

/// Map one raw contact through calibration and geometry.
///
/// The order is fixed and observable: swap the axes first, then mirror them.
/// `--probe-touch` documents that order to the operator because a swap applied
/// after a flip mirrors the other axis and silently produces a plausible but
/// wrong calibration.
fn map_contact(
    calibration: Calibration,
    x_range: AxisRange,
    y_range: AxisRange,
    slot: usize,
    contact: Contact,
    geometry: &Geometry,
) -> ContactReport {
    let (mut panel_x, mut panel_y) = if calibration.swap_xy {
        (
            normalize(contact.y, y_range, geometry.panel_w),
            normalize(contact.x, x_range, geometry.panel_h),
        )
    } else {
        (
            normalize(contact.x, x_range, geometry.panel_w),
            normalize(contact.y, y_range, geometry.panel_h),
        )
    };
    if calibration.flip_x {
        panel_x = geometry.panel_w - 1 - panel_x;
    }
    if calibration.flip_y {
        panel_y = geometry.panel_h - 1 - panel_y;
    }
    let (logical_x, logical_y) = geometry.panel_to_logical(panel_x, panel_y);
    ContactReport {
        slot,
        raw_x: contact.x,
        raw_y: contact.y,
        panel_x,
        panel_y,
        logical_x,
        logical_y,
    }
}

struct Device {
    path: String,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    file: File,
    grabbed: bool,
    x_range: AxisRange,
    y_range: AxisRange,
    state: TouchState,
}

impl Device {
    #[cfg(target_os = "linux")]
    fn grab(&mut self) -> Result<()> {
        use std::os::fd::AsRawFd;

        if self.grabbed {
            return Ok(());
        }
        set_evdev_grab(self.file.as_raw_fd(), true)
            .with_context(|| format!("taking exclusive ownership of {}", self.path))?;
        self.grabbed = true;
        log::info!("kobo input: grabbed {} exclusively", self.path);
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    fn grab(&mut self) -> Result<()> {
        bail!("exclusive evdev ownership is only supported on Linux")
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.grabbed {
            return;
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::fd::AsRawFd;

            match set_evdev_grab(self.file.as_raw_fd(), false) {
                Ok(()) => log::info!("kobo input: released exclusive grab on {}", self.path),
                Err(error) => {
                    log::error!(
                        "kobo input: failed to release grab on {}: {error}",
                        self.path
                    )
                }
            }
        }
        self.grabbed = false;
    }
}

pub struct Input {
    devices: Vec<Device>,
    calibration: Calibration,
}

impl Input {
    pub fn discover() -> Result<Self> {
        let mut devices = discover_devices()?;
        let overrides = RangeOverride::from_env()?;
        if !overrides.is_empty() {
            for device in &mut devices {
                let (x_range, y_range) = overrides.apply(device.x_range, device.y_range)?;
                log::info!(
                    "kobo input: {} axis override x={}..{} y={}..{} (declared x={}..{} y={}..{})",
                    device.path,
                    x_range.min,
                    x_range.max,
                    y_range.min,
                    y_range.max,
                    device.x_range.min,
                    device.x_range.max,
                    device.y_range.min,
                    device.y_range.max
                );
                device.x_range = x_range;
                device.y_range = y_range;
            }
        }
        Ok(Self {
            devices,
            calibration: Calibration::from_env(),
        })
    }

    /// Take exclusive ownership of the touchscreen selected by capability
    /// discovery. Probe mode deliberately does not call this method.
    pub fn grab_selected(&mut self) -> Result<()> {
        let Some(device) = self.devices.first_mut() else {
            bail!(
                "PocketJS Kobo Glo runtime requires a discoverable evdev touchscreen; \
                 no device exposed multitouch X/Y axes or BTN_TOUCH with ABS_X/ABS_Y"
            );
        };
        device.grab()
    }

    pub fn poll_touches(&mut self, geometry: &Geometry) -> Result<Vec<u32>> {
        // The framework wire reserves eight bits for contact identity. Linux
        // MT slots are already stable for a contact's lifetime and this host
        // caps them at eight, so they cannot collide the way a truncated
        // kernel tracking id could.
        Ok(self
            .poll_reports(geometry)?
            .into_iter()
            .map(|report| pack_touch(report.slot as u32, report.logical_x, report.logical_y))
            .collect())
    }

    /// Drain the touchscreen and report the active contacts in raw, panel and
    /// logical coordinates. `poll_touches` is this with the wire packing
    /// applied; `--probe-touch` consumes the untruncated form.
    pub fn poll_reports(&mut self, geometry: &Geometry) -> Result<Vec<ContactReport>> {
        for device in &mut self.devices {
            read_events(device)?;
        }
        // A Kobo normally has one touchscreen. If firmware exposes the same
        // panel through multiple nodes, using the first capable node avoids
        // duplicate contacts.
        let Some(device) = self.devices.first() else {
            return Ok(Vec::new());
        };
        Ok(device
            .state
            .contacts
            .iter()
            .enumerate()
            .filter(|(_, contact)| contact.active)
            .map(|(slot, contact)| {
                map_contact(
                    self.calibration,
                    device.x_range,
                    device.y_range,
                    slot,
                    *contact,
                    geometry,
                )
            })
            .collect())
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// The axis contract of the node `grab_selected` would claim, or `None`
    /// when discovery found no touchscreen.
    pub fn selected_axes(&self) -> Option<TouchAxes> {
        self.devices.first().map(|device| TouchAxes {
            path: device.path.clone(),
            multitouch: device.state.mt,
            x_min: device.x_range.min,
            x_max: device.x_range.max,
            y_min: device.y_range.min,
            y_max: device.y_range.max,
        })
    }

    /// The calibration currently in force, as the environment variables that
    /// would reproduce it. The axis extents are the ones actually in use, so a
    /// probe session shows the overridden values rather than the declared ones.
    pub fn calibration_summary(&self) -> String {
        let axes = self.selected_axes();
        format!(
            "POCKETJS_TOUCH_SWAP_XY={} POCKETJS_TOUCH_FLIP_X={} POCKETJS_TOUCH_FLIP_Y={} \
             POCKETJS_TOUCH_X_MIN={} POCKETJS_TOUCH_X_MAX={} \
             POCKETJS_TOUCH_Y_MIN={} POCKETJS_TOUCH_Y_MAX={}",
            u8::from(self.calibration.swap_xy),
            u8::from(self.calibration.flip_x),
            u8::from(self.calibration.flip_y),
            axes.as_ref().map_or(0, |axes| axes.x_min),
            axes.as_ref().map_or(0, |axes| axes.x_max),
            axes.as_ref().map_or(0, |axes| axes.y_min),
            axes.as_ref().map_or(0, |axes| axes.y_max)
        )
    }
}

fn normalize(value: i32, range: AxisRange, extent: usize) -> usize {
    if extent <= 1 || range.max <= range.min {
        return 0;
    }
    let value = value.clamp(range.min, range.max) - range.min;
    let denominator = (range.max - range.min) as i64;
    ((value as i64 * (extent - 1) as i64 + denominator / 2) / denominator) as usize
}

/// framework/src/touch.ts legacy form: `(id:8 << 18) | (y:9 << 9) | x:9`.
fn pack_touch(id: u32, x: u32, y: u32) -> u32 {
    debug_assert!(x <= 511 && y <= 511);
    ((id & TOUCH_ID_MASK) << 18) | ((y & 0x1ff) << 9) | (x & 0x1ff)
}

#[cfg(target_os = "linux")]
fn discover_devices() -> Result<Vec<Device>> {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let mut paths = std::fs::read_dir("/dev/input")
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("event"))
        })
        .collect::<Vec<_>>();
    paths.sort();

    let mut devices = Vec::new();
    for path in paths {
        let Ok(file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&path)
        else {
            continue;
        };
        let fd = file.as_raw_fd();
        let mt_x = read_abs_range(fd, ABS_MT_POSITION_X);
        let mt_y = read_abs_range(fd, ABS_MT_POSITION_Y);
        let single_x = read_abs_range(fd, ABS_X);
        let single_y = read_abs_range(fd, ABS_Y);
        let (x_range, y_range, mt) = match (mt_x, mt_y, single_x, single_y) {
            (Some(x), Some(y), _, _) => (x, y, true),
            (_, _, Some(x), Some(y)) if has_event_code(fd, EV_KEY, BTN_TOUCH) => (x, y, false),
            _ => continue,
        };
        let path = path.display().to_string();
        log::info!(
            "kobo input: {path}, {}-touch axes x={}..{}, y={}..{}",
            if mt { "multi" } else { "single" },
            x_range.min,
            x_range.max,
            y_range.min,
            y_range.max
        );
        devices.push(Device {
            path,
            file,
            grabbed: false,
            x_range,
            y_range,
            state: TouchState::new(mt),
        });
    }
    // Prefer a true MT touchscreen over single-axis fallback devices,
    // regardless of firmware-specific /dev/input/event numbering.
    devices.sort_by_key(|device| !device.state.mt);
    Ok(devices)
}

#[cfg(not(target_os = "linux"))]
fn discover_devices() -> Result<Vec<Device>> {
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
fn read_abs_range(fd: libc::c_int, axis: u16) -> Option<AxisRange> {
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct InputAbsInfo {
        value: i32,
        minimum: i32,
        maximum: i32,
        fuzz: i32,
        flat: i32,
        resolution: i32,
    }

    // Linux _IOR('E', 0x40 + axis, struct input_absinfo).
    let request = ((2u32 << 30)
        | ((b'E' as u32) << 8)
        | (0x40 + axis as u32)
        | ((std::mem::size_of::<InputAbsInfo>() as u32) << 16)) as libc::c_ulong;
    let mut info = InputAbsInfo::default();
    // SAFETY: request writes one InputAbsInfo into a valid pointer.
    if unsafe { libc::ioctl(fd, request as _, &mut info) } < 0 || info.maximum <= info.minimum {
        return None;
    }
    Some(AxisRange {
        min: info.minimum,
        max: info.maximum,
    })
}

#[cfg(target_os = "linux")]
fn has_event_code(fd: libc::c_int, event_type: u16, code: u16) -> bool {
    let mut bits = [0u8; 96];
    // Linux EVIOCGBIT(event_type, len).
    let request = ((2u32 << 30)
        | ((b'E' as u32) << 8)
        | (0x20 + event_type as u32)
        | ((bits.len() as u32) << 16)) as libc::c_ulong;
    // SAFETY: ioctl writes at most the encoded byte length into `bits`.
    let written = unsafe { libc::ioctl(fd, request as _, bits.as_mut_ptr()) };
    if written < 0 {
        return false;
    }
    let byte = code as usize / 8;
    byte < written as usize && bits[byte] & (1 << (code % 8)) != 0
}

#[cfg(target_os = "linux")]
fn set_evdev_grab(fd: libc::c_int, grab: bool) -> Result<()> {
    // Linux _IOW('E', 0x90, int). EVIOCGRAB takes 1 to exclude every other
    // reader and 0 to release ownership.
    let request = ((1u32 << 30)
        | ((b'E' as u32) << 8)
        | 0x90
        | ((std::mem::size_of::<libc::c_int>() as u32) << 16)) as libc::c_ulong;
    let value: libc::c_int = i32::from(grab);
    // SAFETY: fd is an open evdev descriptor and EVIOCGRAB consumes the
    // immediate integer value without dereferencing a userspace pointer.
    if unsafe { libc::ioctl(fd, request as _, value) } < 0 {
        return Err(std::io::Error::last_os_error()).context(if grab {
            "EVIOCGRAB(1) failed"
        } else {
            "EVIOCGRAB(0) failed"
        });
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_events(device: &mut Device) -> Result<()> {
    use std::mem::size_of;
    use std::os::fd::AsRawFd;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct InputEvent {
        time: libc::timeval,
        type_: u16,
        code: u16,
        value: i32,
    }

    let event_size = size_of::<InputEvent>();
    let mut bytes = [0u8; 64 * 24];
    loop {
        // SAFETY: `bytes` is writable for its full declared length and fd is
        // nonblocking. The kernel writes a sequence of input_event records.
        let count = unsafe {
            libc::read(
                device.file.as_raw_fd(),
                bytes.as_mut_ptr().cast(),
                bytes.len(),
            )
        };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                break;
            }
            return Err(error.into());
        }
        if count == 0 {
            break;
        }
        let count = count as usize;
        if count % event_size != 0 {
            log::warn!(
                "kobo input: ignored partial event read of {count} bytes (record {event_size})"
            );
        }
        for chunk in bytes[..count - count % event_size].chunks_exact(event_size) {
            // SAFETY: chunk has exactly InputEvent bytes; read_unaligned avoids
            // imposing alignment on the byte buffer.
            let event = unsafe { chunk.as_ptr().cast::<InputEvent>().read_unaligned() };
            log::debug!(
                "kobo input event: type={} code={} value={} record_size={event_size}",
                event.type_,
                event.code,
                event.value
            );
            if device.state.apply(event.type_, event.code, event.value) {
                log::warn!(
                    "kobo input: event stream dropped; cleared contacts and resynchronizing"
                );
            }
        }
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn read_events(_device: &mut Device) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_clamps_and_rounds_endpoints() {
        let range = AxisRange {
            min: 100,
            max: 1100,
        };
        assert_eq!(normalize(0, range, 758), 0);
        assert_eq!(normalize(600, range, 758), 379);
        assert_eq!(normalize(2000, range, 758), 757);
    }

    #[test]
    fn multitouch_slots_pack_wire_contacts() {
        let mut state = TouchState::new(true);
        state.apply(EV_ABS, ABS_MT_SLOT, 2);
        state.apply(EV_ABS, ABS_MT_TRACKING_ID, 0x1234);
        state.apply(EV_ABS, ABS_MT_POSITION_X, 511);
        state.apply(EV_ABS, ABS_MT_POSITION_Y, 412);
        let contact = state.contacts[2];
        assert!(contact.active);
        assert_eq!(pack_touch(2, 511, 412), (2 << 18) | (412 << 9) | 511);
        state.apply(EV_ABS, ABS_MT_TRACKING_ID, -1);
        assert!(!state.contacts[2].active);
    }

    #[test]
    fn invalid_multitouch_slots_are_ignored_instead_of_aliased() {
        let mut state = TouchState::new(true);
        state.apply(EV_ABS, ABS_MT_SLOT, 2);
        state.apply(EV_ABS, ABS_MT_TRACKING_ID, 12);
        state.apply(EV_ABS, ABS_MT_POSITION_X, 10);
        state.apply(EV_ABS, ABS_MT_SLOT, MAX_CONTACTS as i32 + 2);
        state.apply(EV_ABS, ABS_MT_TRACKING_ID, 99);
        state.apply(EV_ABS, ABS_MT_POSITION_X, 200);
        assert!(state.contacts[2].active);
        assert_eq!(state.contacts[2].id, 12);
        assert_eq!(state.contacts[2].x, 10);
        assert_eq!(
            state
                .contacts
                .iter()
                .filter(|contact| contact.active)
                .count(),
            1
        );
    }

    #[test]
    fn syn_dropped_clears_contacts_and_discards_until_report() {
        let mut state = TouchState::new(true);
        state.apply(EV_ABS, ABS_MT_SLOT, 1);
        state.apply(EV_ABS, ABS_MT_TRACKING_ID, 42);
        assert!(state.contacts[1].active);

        assert!(state.apply(EV_SYN, SYN_DROPPED, 0));
        assert!(state.contacts.iter().all(|contact| !contact.active));
        state.apply(EV_ABS, ABS_MT_SLOT, 1);
        state.apply(EV_ABS, ABS_MT_TRACKING_ID, 77);
        state.apply(EV_SYN, SYN_REPORT, 0);
        assert!(state.contacts.iter().all(|contact| !contact.active));

        state.apply(EV_ABS, ABS_MT_SLOT, 1);
        state.apply(EV_ABS, ABS_MT_TRACKING_ID, 77);
        assert!(state.contacts[1].active);
        assert_eq!(state.contacts[1].id, 77);
    }

    #[test]
    fn single_touch_down_and_up_are_stateful() {
        let mut state = TouchState::new(false);
        state.apply(EV_ABS, ABS_X, 10);
        state.apply(EV_ABS, ABS_Y, 20);
        state.apply(EV_KEY, BTN_TOUCH, 1);
        assert!(state.contacts[0].active);
        state.apply(EV_KEY, BTN_TOUCH, 0);
        assert!(!state.contacts[0].active);
    }

    fn glo_geometry() -> Geometry {
        Geometry::exact(379, 512, 2, 758, 1024, None).expect("Glo geometry")
    }

    fn corner(calibration: Calibration, raw_x: i32, raw_y: i32) -> (u32, u32) {
        let range = AxisRange { min: 0, max: 1000 };
        let contact = Contact {
            active: true,
            id: 0,
            x: raw_x,
            y: raw_y,
        };
        let report = map_contact(calibration, range, range, 0, contact, &glo_geometry());
        (report.logical_x, report.logical_y)
    }

    #[test]
    fn calibration_swaps_before_it_mirrors() {
        // Applying the mirror first would flip the other axis once the swap
        // moved it, so the two orders disagree wherever both are enabled.
        let swap_then_flip = Calibration {
            swap_xy: true,
            flip_x: true,
            flip_y: false,
        };
        // Raw origin: swapped it is still the origin, then x mirrors to the
        // far edge while y stays at 0.
        assert_eq!(corner(swap_then_flip, 0, 0), (378, 0));
        // Raw x extreme becomes the y extreme after the swap.
        assert_eq!(corner(swap_then_flip, 1000, 0), (378, 511));
    }

    #[test]
    fn identity_calibration_maps_raw_extremes_to_logical_extremes() {
        let identity = Calibration::default();
        assert_eq!(corner(identity, 0, 0), (0, 0));
        assert_eq!(corner(identity, 1000, 1000), (378, 511));
        assert_eq!(corner(identity, 500, 500), (189, 256));
    }

    #[test]
    fn every_probe_report_carries_the_raw_values_that_produced_it() {
        let report = map_contact(
            Calibration::default(),
            AxisRange { min: 0, max: 1000 },
            AxisRange { min: 0, max: 1000 },
            3,
            Contact {
                active: true,
                id: 7,
                x: 250,
                y: 750,
            },
            &glo_geometry(),
        );
        assert_eq!(report.slot, 3);
        assert_eq!((report.raw_x, report.raw_y), (250, 750));
        assert_eq!((report.panel_x, report.panel_y), (189, 767));
        assert_eq!((report.logical_x, report.logical_y), (94, 383));
    }

    #[test]
    fn an_axis_override_replaces_only_the_bounds_it_names() {
        let declared = AxisRange { min: 0, max: 1200 };
        let other = AxisRange { min: 0, max: 1600 };
        let overrides = RangeOverride {
            x_max: Some(1023),
            y_max: Some(757),
            ..RangeOverride::default()
        };
        let (x, y) = overrides.apply(declared, other).expect("override applies");
        assert_eq!(x, AxisRange { min: 0, max: 1023 });
        assert_eq!(y, AxisRange { min: 0, max: 757 });
    }

    #[test]
    fn an_override_that_would_collapse_an_axis_is_rejected() {
        let range = AxisRange { min: 0, max: 1200 };
        let overrides = RangeOverride {
            x_min: Some(900),
            x_max: Some(100),
            ..RangeOverride::default()
        };
        let error = overrides.apply(range, range).unwrap_err().to_string();
        assert!(error.contains("touch x range override is empty"));
    }

    #[test]
    fn the_glo_digitizer_reaches_both_screen_edges_once_overridden() {
        // Measured on a Kobo Glo: the zForce reports panel pixels while
        // declaring 0..1200 by 0..1600, and its axes are swapped and X is
        // mirrored. Under the declaration a left-edge touch lands mid-screen.
        let declared_x = AxisRange { min: 0, max: 1200 };
        let declared_y = AxisRange { min: 0, max: 1600 };
        let calibration = Calibration {
            swap_xy: true,
            flip_x: true,
            flip_y: false,
        };
        let contact = |x, y| Contact {
            active: true,
            id: 0,
            x,
            y,
        };
        let geometry = glo_geometry();

        let declared_left = map_contact(
            calibration,
            declared_x,
            declared_y,
            0,
            contact(0, 757),
            &geometry,
        );
        assert_eq!(declared_left.logical_x, 199);

        let overrides = RangeOverride {
            x_max: Some(1023),
            y_max: Some(757),
            ..RangeOverride::default()
        };
        let (x_range, y_range) = overrides.apply(declared_x, declared_y).expect("override");
        let corner = |x, y| map_contact(calibration, x_range, y_range, 0, contact(x, y), &geometry);
        assert_eq!((corner(0, 757).logical_x, corner(0, 757).logical_y), (0, 0));
        assert_eq!((corner(0, 0).logical_x, corner(0, 0).logical_y), (378, 0));
        assert_eq!(
            (corner(1023, 757).logical_x, corner(1023, 757).logical_y),
            (0, 511)
        );
        assert_eq!(
            (corner(1023, 0).logical_x, corner(1023, 0).logical_y),
            (378, 511)
        );
    }

    #[test]
    fn runtime_requires_a_discoverable_touchscreen() {
        let mut input = Input {
            devices: Vec::new(),
            calibration: Calibration::default(),
        };
        let error = input.grab_selected().unwrap_err().to_string();
        assert!(error.contains("Glo runtime requires a discoverable evdev touchscreen"));
    }
}
