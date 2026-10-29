//! Pen pressure from Linux evdev tablets (Wacom driver or OpenTabletDriver).
//!
//! Position and clicks already reach the app through the normal OS pointer
//! (the Wacom X driver and OTD's mouse emulation both move it). This module
//! only adds what egui/winit cannot see on X11: `ABS_PRESSURE`.
//!
//! The reader is read-only, never grabs the device, and tolerates the device
//! disappearing (OTD restart, tablet unplug). Without permission it reports
//! `PermissionDenied` so the UI can tell the user to join the `input` group.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// What the UI should tell the user about pen detection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PenStatus {
    /// A tablet was found and is readable.
    Ready(String),
    /// No device with ABS_PRESSURE was found.
    NoDevice,
    /// `/dev/input/event*` exists but is not readable by this user.
    PermissionDenied,
    /// This platform has no evdev backend.
    Unsupported,
}

#[derive(Default)]
struct Shared {
    /// Pressure scaled to 0..=1, as f32 bits.
    pressure: AtomicU32,
    /// True while the tip is down (or pressure > 0).
    active: AtomicBool,
    name: Mutex<Option<String>>,
    denied: AtomicBool,
    ready: AtomicBool,
}

impl Shared {
    fn set(&self, value: i32, max: i32, tip: bool) {
        let p = normalize(value, max);
        self.pressure.store(p.to_bits(), Ordering::Relaxed);
        self.active.store(tip || p > 0.0, Ordering::Relaxed);
        self.ready.store(true, Ordering::Relaxed);
    }

    fn clear(&self) {
        self.pressure.store(0.0f32.to_bits(), Ordering::Relaxed);
        self.active.store(false, Ordering::Relaxed);
    }
}

pub struct PenReader {
    shared: Arc<Shared>,
}

impl PenReader {
    /// Start the background reader. Never fails: a missing device or missing
    /// permission is reported through [`PenReader::status`].
    pub fn start() -> Self {
        let shared = Arc::new(Shared::default());
        #[cfg(target_os = "linux")]
        {
            let sh = shared.clone();
            std::thread::Builder::new()
                .name("vixel-pen".into())
                .spawn(move || run(sh))
                .ok();
        }
        PenReader { shared }
    }

    /// A reader that does nothing (tests, platforms without evdev).
    pub fn disabled() -> Self {
        PenReader {
            shared: Arc::new(Shared::default()),
        }
    }

    pub fn status(&self) -> PenStatus {
        if !cfg!(target_os = "linux") {
            return PenStatus::Unsupported;
        }
        if let Some(name) = self.shared.name.lock().ok().and_then(|n| n.clone())
            && self.shared.ready.load(Ordering::Relaxed)
        {
            return PenStatus::Ready(name);
        }
        if self.shared.denied.load(Ordering::Relaxed) {
            return PenStatus::PermissionDenied;
        }
        PenStatus::NoDevice
    }

    /// Current tip pressure while drawing, else `None` (so a mouse stroke is
    /// never throttled to the pen's idle pressure).
    pub fn pressure(&self) -> Option<f32> {
        if !self.shared.active.load(Ordering::Relaxed) {
            return None;
        }
        Some(f32::from_bits(self.shared.pressure.load(Ordering::Relaxed)))
    }

    /// Last pressure seen, even with the tip up (for `:pen` diagnostics).
    pub fn last_pressure(&self) -> Option<f32> {
        if !self.shared.ready.load(Ordering::Relaxed) {
            return None;
        }
        Some(f32::from_bits(self.shared.pressure.load(Ordering::Relaxed)))
    }
}

impl Default for PenReader {
    fn default() -> Self {
        PenReader::disabled()
    }
}

fn normalize(value: i32, max: i32) -> f32 {
    if max <= 0 {
        return 0.0;
    }
    (value.clamp(0, max) as f32 / max as f32).clamp(0.0, 1.0)
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use evdev::raw_stream::RawDevice;
    use evdev::{AbsoluteAxisCode, EventType, KeyCode};

    pub(super) fn run(shared: Arc<Shared>) {
        loop {
            let Some((device, max, name)) = pick_device(&shared) else {
                std::thread::sleep(Duration::from_secs(2));
                continue;
            };
            if let Ok(mut slot) = shared.name.lock() {
                *slot = Some(name);
            }
            shared.ready.store(true, Ordering::Relaxed);
            read_device(device, max, &shared);
            shared.ready.store(false, Ordering::Relaxed);
            shared.clear();
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn pick_device(shared: &Shared) -> Option<(RawDevice, i32, String)> {
        let entries = std::fs::read_dir("/dev/input").ok()?;
        let mut best: Option<(u8, RawDevice, i32, String)> = None;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("event") {
                continue;
            }
            let path = entry.path();
            let device = match RawDevice::open(&path) {
                Ok(d) => d,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    shared.denied.store(true, Ordering::Relaxed);
                    continue;
                }
                Err(_) => continue,
            };
            let Some(axes) = device.supported_absolute_axes() else {
                continue;
            };
            if !axes.contains(AbsoluteAxisCode::ABS_PRESSURE) {
                continue;
            }
            let Some(max) = device
                .get_absinfo()
                .ok()
                .and_then(|it| {
                    it.into_iter()
                        .find(|(a, _)| *a == AbsoluteAxisCode::ABS_PRESSURE)
                })
                .map(|(_, info)| info.maximum())
            else {
                continue;
            };
            let name = device.name().unwrap_or("tablet").to_string();
            let priority = if name.to_ascii_lowercase().contains("opentabletdriver") {
                3
            } else if name.to_ascii_lowercase().contains("wacom") {
                2
            } else if ["pen", "stylus", "tablet"]
                .iter()
                .any(|k| name.to_ascii_lowercase().contains(k))
            {
                1
            } else {
                0
            };
            if best.as_ref().is_none_or(|(p, ..)| priority > *p) {
                best = Some((priority, device, max.max(1), name));
            }
        }
        best.map(|(_, d, m, n)| (d, m, n))
    }

    fn read_device(mut device: RawDevice, max: i32, shared: &Shared) {
        let mut pressure = 0i32;
        let mut touch = false;
        loop {
            let events = match device.fetch_events() {
                Ok(events) => events,
                Err(_) => return,
            };
            let mut changed = false;
            for event in events {
                match event.event_type() {
                    EventType::ABSOLUTE if event.code() == AbsoluteAxisCode::ABS_PRESSURE.0 => {
                        pressure = event.value();
                        changed = true;
                    }
                    EventType::KEY if event.code() == KeyCode::BTN_TOUCH.0 => {
                        touch = event.value() != 0;
                        changed = true;
                    }
                    _ => {}
                }
            }
            if changed {
                shared.set(pressure, max, touch);
            }
        }
    }
}

#[cfg(target_os = "linux")]
use linux::run;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_normalizes_and_clamps() {
        assert_eq!(normalize(0, 8191), 0.0);
        assert_eq!(normalize(8191, 8191), 1.0);
        assert!((normalize(4095, 8191) - 0.4999).abs() < 0.01);
        assert_eq!(normalize(9999, 8191), 1.0);
        assert_eq!(normalize(10, 0), 0.0);
    }

    #[test]
    fn disabled_reader_reports_nothing() {
        let pen = PenReader::disabled();
        assert_eq!(pen.pressure(), None);
        assert!(matches!(
            pen.status(),
            PenStatus::NoDevice | PenStatus::Unsupported
        ));
    }
}
