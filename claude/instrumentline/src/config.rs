use std::env;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::theme::glyphs::GlyphTable;

pub const CONFIGURATION_PATH_VARIABLE: &str = "INSTRUMENTLINE_CONFIG";
pub const STATE_DIRECTORY_VARIABLE: &str = "INSTRUMENTLINE_STATE_DIR";
pub const TERMINAL_COLUMNS_VARIABLE: &str = "COLUMNS";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowSlotStrategy {
    AlternateWindows,
    ShowWorstWindow,
    #[default]
    PreferFiveHourUntilSevenDayMatters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HealthWeights {
    pub context: f32,
    pub five_hour_window: f32,
    pub cache_hit_ratio: f32,
    pub burn_rate: f32,
}

impl Default for HealthWeights {
    fn default() -> Self {
        Self {
            context: 0.40,
            five_hour_window: 0.26,
            cache_hit_ratio: 0.26,
            burn_rate: 0.20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HealthRamps {
    pub context_from: f32,
    pub context_to: f32,
    pub five_hour_from: f32,
    pub five_hour_to: f32,
    pub cache_hit_healthy: f32,
    pub cache_hit_poor: f32,
    pub burn_from_thousands_per_minute: f32,
    pub burn_to_thousands_per_minute: f32,
}

impl Default for HealthRamps {
    fn default() -> Self {
        Self {
            context_from: 38.0,
            context_to: 83.0,
            five_hour_from: 45.0,
            five_hour_to: 90.0,
            cache_hit_healthy: 0.82,
            cache_hit_poor: 0.40,
            burn_from_thousands_per_minute: 3.5,
            burn_to_thousands_per_minute: 14.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AlertThresholds {
    pub cache_hit_warning: f32,
    pub cache_hit_critical: f32,
    pub burn_thousands_per_minute: f32,
    pub long_session_minutes: u64,
    pub context_notice_percentage: f32,
    pub window_notice_percentage: f32,
    pub window_critical_percentage: f32,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            cache_hit_warning: 0.60,
            cache_hit_critical: 0.45,
            burn_thousands_per_minute: 8.0,
            long_session_minutes: 180,
            context_notice_percentage: 75.0,
            window_notice_percentage: 75.0,
            window_critical_percentage: 90.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Configuration {
    pub glyphs: GlyphTable,
    pub easing_alpha: f32,
    pub auto_compact_enabled: bool,
    pub compaction_threshold_percentage: f32,
    pub window_slot_strategy: WindowSlotStrategy,
    pub health_weights: HealthWeights,
    pub health_ramps: HealthRamps,
    pub alert_thresholds: AlertThresholds,
    pub alert_rotation_frames: u64,
    pub minimum_columns: usize,
    pub fallback_columns: usize,
    pub dollars_per_thousand_tokens: f32,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            glyphs: GlyphTable::default(),
            easing_alpha: 0.60,
            auto_compact_enabled: true,
            compaction_threshold_percentage: 92.0,
            window_slot_strategy: WindowSlotStrategy::default(),
            health_weights: HealthWeights::default(),
            health_ramps: HealthRamps::default(),
            alert_thresholds: AlertThresholds::default(),
            alert_rotation_frames: 5,
            minimum_columns: 40,
            fallback_columns: 100,
            dollars_per_thousand_tokens: 0.42,
        }
    }
}

impl Configuration {
    #[must_use]
    pub fn load_from_environment() -> Self {
        Self::discover_path()
            .and_then(|path| Self::load_from_path(&path))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn load_from_path(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    #[must_use]
    pub fn discover_path() -> Option<PathBuf> {
        if let Ok(explicit) = env::var(CONFIGURATION_PATH_VARIABLE)
            && !explicit.trim().is_empty()
        {
            return Some(PathBuf::from(explicit));
        }
        let home = env::var("HOME").ok()?;
        let candidate = PathBuf::from(home)
            .join(".claude")
            .join("instrumentline.json");
        candidate.exists().then_some(candidate)
    }

    #[must_use]
    pub fn state_directory() -> PathBuf {
        if let Ok(explicit) = env::var(STATE_DIRECTORY_VARIABLE)
            && !explicit.trim().is_empty()
        {
            return PathBuf::from(explicit);
        }
        env::var("HOME").map_or_else(
            |_| PathBuf::from("/tmp/instrumentline"),
            |home| {
                PathBuf::from(home)
                    .join(".claude")
                    .join("instrumentline-state")
            },
        )
    }

    #[must_use]
    pub fn terminal_columns(&self) -> usize {
        env::var(TERMINAL_COLUMNS_VARIABLE)
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok())
            .filter(|columns| *columns >= self.minimum_columns)
            .unwrap_or(self.fallback_columns)
    }

    #[must_use]
    pub const fn easing_alpha_clamped(&self) -> f32 {
        self.easing_alpha.clamp(0.05, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_json() {
        let original = Configuration::default();
        let encoded = serde_json::to_string(&original).unwrap();
        let decoded: Configuration = serde_json::from_str(&encoded).unwrap();
        assert!((decoded.easing_alpha - original.easing_alpha).abs() < f32::EPSILON);
        assert_eq!(decoded.window_slot_strategy, original.window_slot_strategy);
        assert_eq!(decoded.glyphs.model_tiers, original.glyphs.model_tiers);
    }

    #[test]
    fn partial_configuration_files_fill_in_defaults() {
        let decoded: Configuration =
            serde_json::from_str(r#"{"easing_alpha": 0.9, "auto_compact_enabled": false}"#)
                .unwrap();
        assert!((decoded.easing_alpha - 0.9).abs() < f32::EPSILON);
        assert!(!decoded.auto_compact_enabled);
        assert!((decoded.compaction_threshold_percentage - 92.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unknown_configuration_keys_are_rejected() {
        let outcome: Result<Configuration, serde_json::Error> =
            serde_json::from_str(r#"{"nonsense": 1}"#);
        assert!(outcome.is_err());
    }

    #[test]
    fn easing_alpha_is_clamped_into_a_usable_range() {
        let loud = Configuration {
            easing_alpha: 9.0,
            ..Configuration::default()
        };
        assert!((loud.easing_alpha_clamped() - 1.0).abs() < f32::EPSILON);
        let silent = Configuration {
            easing_alpha: 0.0,
            ..Configuration::default()
        };
        assert!((silent.easing_alpha_clamped() - 0.05).abs() < f32::EPSILON);
    }

    #[test]
    fn missing_configuration_path_yields_defaults() {
        assert!(Configuration::load_from_path(Path::new("/nonexistent/x.json")).is_none());
    }
}
