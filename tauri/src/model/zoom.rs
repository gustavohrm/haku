//! Zoom levels kept per site. [Zoom](../../../docs/specs/zoom.md).

use std::ops::{Deref, DerefMut};

use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use super::tab::host_of;

/// A page's zoom when nothing else is chosen, in percent.
pub const DEFAULT_ZOOM: u16 = 100;
/// The least zoom WebView2 applies, in percent.
pub const MIN_ZOOM: u16 = 25;
/// The most zoom WebView2 applies, in percent.
pub const MAX_ZOOM: u16 = 500;

/// A zoom factor the engine reported, in whole percent within the engine's
/// range. A factor that is not a number reads as the default.
pub fn percent_of(factor: f64) -> u16 {
    if !factor.is_finite() {
        return DEFAULT_ZOOM;
    }
    // Clamped first, so the cast cannot overflow.
    (factor * 100.0).round().clamp(f64::from(MIN_ZOOM), f64::from(MAX_ZOOM)) as u16
}

/// A level kept for a host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SiteZoom {
    /// As [`host_of`] gives it.
    pub host: String,
    pub percent: u16,
}

/// The levels stored in settings. 100% is never stored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Type)]
#[serde(transparent)]
#[specta(inline)]
pub struct SiteZooms(pub Vec<SiteZoom>);

/// Drops any entry that cannot be read, and reads a value that is not a list
/// as none, as [`super::permission::SitePermissions`] does.
impl<'de> Deserialize<'de> for SiteZooms {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let serde_json::Value::Array(entries) = serde_json::Value::deserialize(deserializer)? else {
            return Ok(Self::default());
        };
        Ok(Self(
            entries
                .into_iter()
                .filter_map(|entry| serde_json::from_value(entry).ok())
                .collect(),
        ))
    }
}

impl Deref for SiteZooms {
    type Target = Vec<SiteZoom>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SiteZooms {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl SiteZooms {
    /// Writes hosts as [`host_of`] gives them and levels within the engine's
    /// range, drops any entry without a host or at 100%, and keeps one level
    /// per host, the last one listed.
    pub fn normalized(self) -> Self {
        let mut kept: Vec<SiteZoom> = Vec::new();
        for entry in self.0 {
            let typed = entry.host.trim().to_ascii_lowercase();
            let Some(host) = host_of(&format!("https://{typed}/")).filter(|host| *host == typed) else {
                continue;
            };
            kept.retain(|other| other.host != host);
            let percent = entry.percent.clamp(MIN_ZOOM, MAX_ZOOM);
            if percent != DEFAULT_ZOOM {
                kept.push(SiteZoom { host, percent });
            }
        }
        Self(kept)
    }
}

/// The level listed for a host.
pub fn zoom_for(levels: &[SiteZoom], host: &str) -> Option<u16> {
    levels
        .iter()
        .find(|level| level.host == host)
        .map(|level| level.percent)
}

/// Lists `percent` for `host`, replacing what was listed. At 100% the host is
/// dropped instead when `keep_default` is not set.
pub fn set_zoom(levels: &mut Vec<SiteZoom>, host: &str, percent: u16, keep_default: bool) {
    levels.retain(|level| level.host != host);
    if percent != DEFAULT_ZOOM || keep_default {
        levels.push(SiteZoom {
            host: host.to_string(),
            percent,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(host: &str, percent: u16) -> SiteZoom {
        SiteZoom {
            host: host.into(),
            percent,
        }
    }

    #[test]
    fn a_factor_reads_as_whole_percent_within_the_engines_range() {
        assert_eq!(percent_of(1.25), 125);
        assert_eq!(percent_of(0.333_333), 33);
        assert_eq!(percent_of(0.1), MIN_ZOOM);
        assert_eq!(percent_of(9.0), MAX_ZOOM);
        assert_eq!(percent_of(f64::NAN), DEFAULT_ZOOM);
    }

    #[test]
    fn one_level_is_kept_per_host_the_last_listed_and_never_the_default() {
        let kept = SiteZooms(vec![
            level(" A.Test ", 150),
            level("a.test", 125),
            level("b.test", 100),
            level("c.test", 9000),
            level("https://d.test/", 150),
            level("  ", 150),
        ])
        .normalized();
        assert_eq!(kept.0, vec![level("a.test", 125), level("c.test", MAX_ZOOM)]);
    }

    #[test]
    fn an_unreadable_stored_entry_is_dropped_and_the_rest_kept() {
        let json = serde_json::json!([
            { "host": "a.test", "percent": 150 },
            { "host": "b.test", "percent": -3 },
            "nonsense",
        ]);
        assert_eq!(SiteZooms::deserialize(json).unwrap().0, vec![level("a.test", 150)]);
        assert_eq!(
            SiteZooms::deserialize(serde_json::json!({ "host": "a.test" })).unwrap(),
            SiteZooms::default()
        );
    }

    #[test]
    fn setting_the_default_drops_the_host_unless_it_must_be_kept() {
        let mut levels = vec![level("a.test", 150)];
        set_zoom(&mut levels, "a.test", 100, false);
        assert!(levels.is_empty());
        set_zoom(&mut levels, "a.test", 100, true);
        assert_eq!(levels, vec![level("a.test", 100)]);
        set_zoom(&mut levels, "a.test", 175, false);
        assert_eq!(levels, vec![level("a.test", 175)]);
    }
}
