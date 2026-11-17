use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::KiteError;

/// Required UA fonts for `KiteContext::new` (SPEC v24.2 §5.7 / DD-22, DD-35).
/// Noto Sans SC and monochrome Noto Emoji are required after the 2026-11-16
/// reversal of Q-05. Color-emoji paint is not claimed.
pub const REQUIRED_UA_FONTS: &[&str] = &[
    "Noto Sans",
    "Noto Sans Italic",
    "Noto Serif",
    "Noto Sans Mono",
    "Noto Sans Symbols",
    "Noto Sans SC",
    "Noto Emoji",
];

/// A clock that returns Unix UTC seconds (AT-161 / DD-110).
pub trait Clock {
    fn unix_utc_secs(&self) -> i64;
}

/// The process clock used by `KiteContext::new`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessClock;

impl Clock for ProcessClock {
    fn unix_utc_secs(&self) -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(e) => -(e.duration().as_secs() as i64),
        }
    }
}

/// Holds one UA font set. There is no process-global font state (DD-121).
pub struct KiteContext {
    ua_fonts: Vec<(String, Vec<u8>)>,
    clock: Box<dyn Clock + Send + Sync>,
}

impl KiteContext {
    /// Build a context from required UA font bytes using the process clock.
    pub fn new(
        ua_fonts: impl IntoIterator<Item = (impl Into<String>, impl Into<Vec<u8>>)>,
    ) -> Result<Self, KiteError> {
        Self::new_with_clock(ua_fonts, ProcessClock)
    }

    /// Same as `new`, plus a clock that returns Unix UTC seconds.
    /// Not exported on phone UniFFI (DD-110).
    pub fn new_with_clock(
        ua_fonts: impl IntoIterator<Item = (impl Into<String>, impl Into<Vec<u8>>)>,
        clock: impl Clock + Send + Sync + 'static,
    ) -> Result<Self, KiteError> {
        let ua_fonts: Vec<(String, Vec<u8>)> = ua_fonts
            .into_iter()
            .map(|(name, bytes)| (name.into(), bytes.into()))
            .collect();
        let present: BTreeSet<&str> = ua_fonts
            .iter()
            .filter(|(_, bytes)| !bytes.is_empty())
            .map(|(name, _)| name.as_str())
            .collect();
        let missing: Vec<&str> = REQUIRED_UA_FONTS
            .iter()
            .copied()
            .filter(|name| !present.contains(name))
            .collect();
        if !missing.is_empty() {
            return Err(KiteError::new(
                "missing_ua_font",
                format!("missing required UA fonts: {}", missing.join(", ")),
            ));
        }
        Ok(Self {
            ua_fonts,
            clock: Box::new(clock),
        })
    }

    /// System / host fonts are never used (`system_fonts` stays false).
    pub fn system_fonts(&self) -> bool {
        false
    }

    pub fn unix_utc_secs(&self) -> i64 {
        self.clock.unix_utc_secs()
    }

    pub fn ua_fonts(&self) -> &[(String, Vec<u8>)] {
        &self.ua_fonts
    }
}

impl std::fmt::Debug for KiteContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KiteContext")
            .field(
                "ua_fonts",
                &self
                    .ua_fonts
                    .iter()
                    .map(|(name, bytes)| (name.as_str(), bytes.len()))
                    .collect::<Vec<_>>(),
            )
            .field("system_fonts", &self.system_fonts())
            .finish()
    }
}
