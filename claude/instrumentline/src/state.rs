use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::numeric::interpolate;
use crate::theme::glyphs::PermissionMode;
use crate::transcript::ActivityCounters;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EasedValues {
    pub context_percentage: f32,
    pub five_hour_percentage: f32,
    pub seven_day_percentage: f32,
    pub cache_hit_ratio: f32,
    pub thousand_tokens_per_minute: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    pub frame: u64,
    pub eased: EasedValues,
    pub initialized: bool,
    pub last_sample_epoch_millis: u128,
    pub last_total_cost_usd: f64,
    pub last_total_tokens: u64,
    pub consecutive_cache_write_turns: u32,
    pub last_cache_creation_tokens: u64,
    pub counters: ActivityCounters,
}

#[derive(Debug, Clone, Copy)]
pub struct ObservedSample {
    pub context_percentage: f32,
    pub five_hour_percentage: f32,
    pub seven_day_percentage: f32,
    pub cache_hit_ratio: f32,
    pub thousand_tokens_per_minute: f32,
}

impl SessionState {
    #[must_use]
    pub fn load(state_directory: &Path, session_identifier: &str) -> Self {
        let path = Self::path_for(state_directory, session_identifier);
        fs::read_to_string(path)
            .ok()
            .and_then(|contents| serde_json::from_str(&contents).ok())
            .unwrap_or_default()
    }

    pub fn persist(&self, state_directory: &Path, session_identifier: &str) {
        if fs::create_dir_all(state_directory).is_err() {
            return;
        }
        let path = Self::path_for(state_directory, session_identifier);
        if let Ok(encoded) = serde_json::to_string(self) {
            let temporary = path.with_extension("tmp");
            if fs::write(&temporary, encoded).is_ok() {
                drop(fs::rename(&temporary, &path));
            }
        }
    }

    #[must_use]
    pub fn path_for(state_directory: &Path, session_identifier: &str) -> PathBuf {
        state_directory.join(format!("{}.json", sanitize_identifier(session_identifier)))
    }

    #[must_use]
    pub fn permission_mode_path(state_directory: &Path, session_identifier: &str) -> PathBuf {
        state_directory.join(format!("{}.mode", sanitize_identifier(session_identifier)))
    }

    #[must_use]
    pub fn read_permission_mode(
        state_directory: &Path,
        session_identifier: &str,
    ) -> PermissionMode {
        let path = Self::permission_mode_path(state_directory, session_identifier);
        fs::read_to_string(path)
            .map(|raw| PermissionMode::from_hook_value(raw.trim()))
            .unwrap_or_default()
    }

    pub fn advance_towards(&mut self, sample: ObservedSample, easing_alpha: f32) {
        let alpha = if self.initialized {
            easing_alpha.clamp(0.05, 1.0)
        } else {
            1.0
        };
        self.eased.context_percentage = interpolate(
            self.eased.context_percentage,
            sample.context_percentage,
            alpha,
        );
        self.eased.five_hour_percentage = interpolate(
            self.eased.five_hour_percentage,
            sample.five_hour_percentage,
            alpha,
        );
        self.eased.seven_day_percentage = interpolate(
            self.eased.seven_day_percentage,
            sample.seven_day_percentage,
            alpha,
        );
        self.eased.cache_hit_ratio =
            interpolate(self.eased.cache_hit_ratio, sample.cache_hit_ratio, alpha);
        self.eased.thousand_tokens_per_minute = interpolate(
            self.eased.thousand_tokens_per_minute,
            sample.thousand_tokens_per_minute,
            alpha,
        );
        self.initialized = true;
        self.frame = self.frame.wrapping_add(1);
    }

    pub const fn record_cache_write_turn(&mut self, cache_creation_tokens: u64) {
        if cache_creation_tokens > 0 && cache_creation_tokens != self.last_cache_creation_tokens {
            self.consecutive_cache_write_turns =
                self.consecutive_cache_write_turns.saturating_add(1);
        } else if cache_creation_tokens == 0 {
            self.consecutive_cache_write_turns = 0;
        }
        self.last_cache_creation_tokens = cache_creation_tokens;
    }
}

#[must_use]
pub fn sanitize_identifier(identifier: &str) -> String {
    let cleaned: String = identifier
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "unnamed-session".to_owned()
    } else {
        cleaned
    }
}

#[must_use]
pub fn current_epoch_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(context: f32) -> ObservedSample {
        ObservedSample {
            context_percentage: context,
            five_hour_percentage: 0.0,
            seven_day_percentage: 0.0,
            cache_hit_ratio: 1.0,
            thousand_tokens_per_minute: 0.0,
        }
    }

    #[test]
    fn first_advance_snaps_to_the_sample() {
        let mut state = SessionState::default();
        state.advance_towards(sample(60.0), 0.3);
        assert!((state.eased.context_percentage - 60.0).abs() < 0.001);
        assert_eq!(state.frame, 1);
    }

    #[test]
    fn later_advances_ease_rather_than_jump() {
        let mut state = SessionState::default();
        state.advance_towards(sample(0.0), 0.5);
        state.advance_towards(sample(100.0), 0.5);
        assert!((state.eased.context_percentage - 50.0).abs() < 0.001);
        state.advance_towards(sample(100.0), 0.5);
        assert!((state.eased.context_percentage - 75.0).abs() < 0.001);
    }

    #[test]
    fn easing_converges_within_a_handful_of_frames() {
        let mut state = SessionState::default();
        state.advance_towards(sample(0.0), 0.6);
        for _ in 0..6 {
            state.advance_towards(sample(90.0), 0.6);
        }
        assert!((state.eased.context_percentage - 90.0).abs() < 1.0);
    }

    #[test]
    fn sanitizing_strips_path_traversal_characters() {
        assert_eq!(sanitize_identifier("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitize_identifier(""), "unnamed-session");
        assert_eq!(sanitize_identifier("abc-123"), "abc-123");
    }

    #[test]
    fn sanitized_identifiers_stay_inside_the_state_directory() {
        let path = SessionState::path_for(Path::new("/tmp/state"), "../../escape");
        assert_eq!(path, Path::new("/tmp/state/escape.json"));
    }

    #[test]
    fn cache_write_streak_counts_only_changing_write_totals() {
        let mut state = SessionState::default();
        state.record_cache_write_turn(5000);
        state.record_cache_write_turn(9000);
        assert_eq!(state.consecutive_cache_write_turns, 2);
        state.record_cache_write_turn(9000);
        assert_eq!(state.consecutive_cache_write_turns, 2);
        state.record_cache_write_turn(0);
        assert_eq!(state.consecutive_cache_write_turns, 0);
    }

    #[test]
    fn state_survives_a_round_trip_through_disk() {
        let directory = std::env::temp_dir().join("instrumentline-state-test");
        drop(fs::remove_dir_all(&directory));
        let mut state = SessionState::default();
        state.advance_towards(sample(42.0), 0.6);
        state.persist(&directory, "session-one");
        let reloaded = SessionState::load(&directory, "session-one");
        assert!((reloaded.eased.context_percentage - 42.0).abs() < 0.001);
        assert_eq!(reloaded.frame, 1);
        drop(fs::remove_dir_all(&directory));
    }

    #[test]
    fn missing_state_file_yields_defaults() {
        let state = SessionState::load(Path::new("/nonexistent"), "nope");
        assert_eq!(state.frame, 0);
        assert!(!state.initialized);
    }

    #[test]
    fn missing_mode_file_reads_as_default_mode() {
        let mode = SessionState::read_permission_mode(Path::new("/nonexistent"), "nope");
        assert_eq!(mode, PermissionMode::Default);
    }
}
