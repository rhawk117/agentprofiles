use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct StatusLinePayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub transcript_path: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub model: ModelIdentity,
    #[serde(default)]
    pub workspace: WorkspaceLocation,
    #[serde(default)]
    pub cost: SessionCost,
    #[serde(default)]
    pub context_window: ContextWindow,
    #[serde(default)]
    pub exceeds_200k_tokens: bool,
    #[serde(default)]
    pub fast_mode: bool,
    #[serde(default)]
    pub effort: Option<EffortSetting>,
    #[serde(default)]
    pub thinking: Option<ThinkingSetting>,
    #[serde(default)]
    pub rate_limits: Option<RateLimits>,
    #[serde(default)]
    pub vim: Option<VimState>,
    #[serde(default)]
    pub agent: Option<AgentIdentity>,
    #[serde(default)]
    pub session_name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ModelIdentity {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WorkspaceLocation {
    #[serde(default)]
    pub current_dir: String,
    #[serde(default)]
    pub project_dir: String,
    #[serde(default)]
    pub git_worktree: Option<String>,
    #[serde(default)]
    pub repo: Option<RepositoryIdentity>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RepositoryIdentity {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "field names mirror the documented payload"
)]
pub struct SessionCost {
    #[serde(default)]
    pub total_cost_usd: f64,
    #[serde(default)]
    pub total_duration_ms: u64,
    #[serde(default)]
    pub total_api_duration_ms: u64,
    #[serde(default)]
    pub total_lines_added: u64,
    #[serde(default)]
    pub total_lines_removed: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "field names mirror the documented payload"
)]
pub struct ContextWindow {
    #[serde(default)]
    pub total_input_tokens: u64,
    #[serde(default)]
    pub total_output_tokens: u64,
    #[serde(default = "default_context_window_size")]
    pub context_window_size: u64,
    #[serde(default)]
    pub used_percentage: Option<f64>,
    #[serde(default)]
    pub remaining_percentage: Option<f64>,
    #[serde(default)]
    pub current_usage: Option<CurrentUsage>,
}

impl Default for ContextWindow {
    fn default() -> Self {
        Self {
            total_input_tokens: 0,
            total_output_tokens: 0,
            context_window_size: default_context_window_size(),
            used_percentage: None,
            remaining_percentage: None,
            current_usage: None,
        }
    }
}

const fn default_context_window_size() -> u64 {
    200_000
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[expect(
    clippy::struct_field_names,
    reason = "field names mirror the documented payload"
)]
pub struct CurrentUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EffortSetting {
    #[serde(default)]
    pub level: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct ThinkingSetting {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RateLimits {
    #[serde(default)]
    pub five_hour: Option<RateLimitWindow>,
    #[serde(default)]
    pub seven_day: Option<RateLimitWindow>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct RateLimitWindow {
    #[serde(default)]
    pub used_percentage: f64,
    #[serde(default)]
    pub resets_at: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VimState {
    #[serde(default)]
    pub mode: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AgentIdentity {
    #[serde(default)]
    pub name: String,
}

impl StatusLinePayload {
    #[must_use]
    pub fn parse_lenient(raw_input: &str) -> Self {
        serde_json::from_str(raw_input).unwrap_or_default()
    }

    #[must_use]
    pub fn context_used_percentage(&self) -> f32 {
        if let Some(reported) = self.context_window.used_percentage {
            return crate::numeric::clamp_percentage(reported);
        }
        let window =
            crate::numeric::wide_float_from_tokens(self.context_window.context_window_size.max(1));
        let used = crate::numeric::wide_float_from_tokens(self.context_window.total_input_tokens);
        crate::numeric::clamp_percentage(used * 100.0 / window)
    }

    #[must_use]
    pub fn effort_level(&self) -> &str {
        self.effort
            .as_ref()
            .map_or("", |setting| setting.level.as_str())
    }

    #[must_use]
    pub fn five_hour_percentage(&self) -> Option<f32> {
        self.rate_limits
            .as_ref()
            .and_then(|limits| limits.five_hour)
            .map(|window| crate::numeric::clamp_percentage(window.used_percentage))
    }

    #[must_use]
    pub fn seven_day_percentage(&self) -> Option<f32> {
        self.rate_limits
            .as_ref()
            .and_then(|limits| limits.seven_day)
            .map(|window| crate::numeric::clamp_percentage(window.used_percentage))
    }

    #[must_use]
    pub fn five_hour_reset_epoch(&self) -> Option<u64> {
        self.rate_limits
            .as_ref()
            .and_then(|limits| limits.five_hour)
            .map(|w| w.resets_at)
    }

    #[must_use]
    pub fn seven_day_reset_epoch(&self) -> Option<u64> {
        self.rate_limits
            .as_ref()
            .and_then(|limits| limits.seven_day)
            .map(|w| w.resets_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENTED_SHAPE: &str = r#"{
        "session_id": "abc",
        "version": "2.1.220",
        "model": {"id": "claude-opus-4-8", "display_name": "Opus 4.8"},
        "workspace": {"current_dir": "/home/r/reconflux", "project_dir": "/home/r/reconflux"},
        "cost": {"total_cost_usd": 1.84, "total_duration_ms": 1740000,
                 "total_lines_added": 312, "total_lines_removed": 87},
        "context_window": {"total_input_tokens": 94000, "total_output_tokens": 4200,
                           "context_window_size": 200000, "used_percentage": 47.0,
                           "current_usage": {"input_tokens": 8500, "output_tokens": 1200,
                                             "cache_creation_input_tokens": 5000,
                                             "cache_read_input_tokens": 2000}},
        "exceeds_200k_tokens": false,
        "effort": {"level": "high"},
        "rate_limits": {"five_hour": {"used_percentage": 33.0, "resets_at": 1800000000},
                        "seven_day": {"used_percentage": 19.0, "resets_at": 1800600000}}
    }"#;

    #[test]
    fn parses_the_documented_shape() {
        let payload = StatusLinePayload::parse_lenient(DOCUMENTED_SHAPE);
        assert_eq!(payload.model.display_name, "Opus 4.8");
        assert_eq!(payload.effort_level(), "high");
        assert!((payload.context_used_percentage() - 47.0).abs() < 0.01);
        assert_eq!(payload.five_hour_percentage().map(f32::round), Some(33.0));
        assert_eq!(payload.cost.total_lines_removed, 87);
    }

    #[test]
    fn missing_fields_never_fail_the_parse() {
        let payload = StatusLinePayload::parse_lenient("{}");
        assert_eq!(payload.context_window.context_window_size, 200_000);
        assert_eq!(payload.five_hour_percentage(), None);
        assert_eq!(payload.effort_level(), "");
    }

    #[test]
    fn malformed_input_degrades_to_defaults() {
        let payload = StatusLinePayload::parse_lenient("not json at all");
        assert_eq!(payload.session_id, "");
        assert!(payload.context_used_percentage().abs() < f32::EPSILON);
    }

    #[test]
    fn percentage_is_derived_when_the_field_is_absent() {
        let payload = StatusLinePayload::parse_lenient(
            r#"{"context_window":{"total_input_tokens":50000,"context_window_size":200000}}"#,
        );
        assert!((payload.context_used_percentage() - 25.0).abs() < 0.01);
    }
}
