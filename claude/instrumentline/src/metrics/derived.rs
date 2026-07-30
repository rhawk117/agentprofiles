use crate::numeric::{float_from_tokens, saturating_ratio, wide_float_from_tokens};
use crate::payload::StatusLinePayload;
use crate::state::{ObservedSample, SessionState, current_epoch_millis};

const MILLIS_PER_MINUTE: f64 = 60_000.0;
const MILLIS_PER_HOUR: f64 = 3_600_000.0;
const CONTEXT_GROWTH_PERCENT_PER_MINUTE: f32 = 1.4;

#[derive(Debug, Clone, Copy)]
pub struct DerivedMetrics {
    pub context_percentage: f32,
    pub five_hour_percentage: f32,
    pub seven_day_percentage: f32,
    pub cache_hit_ratio: f32,
    pub thousand_tokens_per_minute: f32,
    pub dollars_per_hour: f32,
    pub used_tokens: u64,
    pub window_tokens: u64,
    pub cache_creation_tokens: u64,
    pub session_minutes: u64,
    pub has_rate_limit_data: bool,
}

impl DerivedMetrics {
    #[must_use]
    pub fn from_payload(payload: &StatusLinePayload, previous: &SessionState) -> Self {
        let usage = payload.context_window.current_usage.unwrap_or_default();
        let cached = float_from_tokens(usage.cache_read_input_tokens);
        let fresh = float_from_tokens(usage.input_tokens);
        let written = float_from_tokens(usage.cache_creation_input_tokens);
        let cache_hit_ratio = saturating_ratio(cached, cached + fresh + written);

        let now_millis = current_epoch_millis();
        let elapsed_millis = now_millis.saturating_sub(previous.last_sample_epoch_millis);
        let total_tokens = payload
            .context_window
            .total_input_tokens
            .saturating_add(payload.context_window.total_output_tokens);

        let thousand_tokens_per_minute = rate_per_minute(
            total_tokens,
            previous.last_total_tokens,
            elapsed_millis,
            previous,
        );
        let dollars_per_hour = spend_per_hour(
            payload.cost.total_cost_usd,
            previous.last_total_cost_usd,
            elapsed_millis,
            previous,
        );

        Self {
            context_percentage: payload.context_used_percentage(),
            five_hour_percentage: payload.five_hour_percentage().unwrap_or(0.0),
            seven_day_percentage: payload.seven_day_percentage().unwrap_or(0.0),
            cache_hit_ratio,
            thousand_tokens_per_minute,
            dollars_per_hour,
            used_tokens: payload.context_window.total_input_tokens,
            window_tokens: payload.context_window.context_window_size.max(1),
            cache_creation_tokens: usage.cache_creation_input_tokens,
            session_minutes: payload.cost.total_duration_ms / 60_000,
            has_rate_limit_data: payload.rate_limits.is_some(),
        }
    }

    #[must_use]
    pub const fn as_sample(&self) -> ObservedSample {
        ObservedSample {
            context_percentage: self.context_percentage,
            five_hour_percentage: self.five_hour_percentage,
            seven_day_percentage: self.seven_day_percentage,
            cache_hit_ratio: self.cache_hit_ratio,
            thousand_tokens_per_minute: self.thousand_tokens_per_minute,
        }
    }

    #[must_use]
    pub fn minutes_until_compaction(&self, threshold_percentage: f32) -> u32 {
        let remaining = threshold_percentage - self.context_percentage;
        if remaining <= 0.0 {
            return 0;
        }
        let minutes = remaining / CONTEXT_GROWTH_PERCENT_PER_MINUTE;
        crate::numeric::percent_from_float(minutes.min(100.0)).max(1)
    }
}

fn rate_per_minute(
    current_tokens: u64,
    previous_tokens: u64,
    elapsed_millis: u128,
    previous: &SessionState,
) -> f32 {
    if !previous.initialized || elapsed_millis == 0 || current_tokens <= previous_tokens {
        return previous.eased.thousand_tokens_per_minute;
    }
    let delta = wide_float_from_tokens(current_tokens.saturating_sub(previous_tokens));
    let minutes = elapsed_to_units(elapsed_millis, MILLIS_PER_MINUTE);
    if minutes <= 0.0 {
        return previous.eased.thousand_tokens_per_minute;
    }
    crate::numeric::clamp_percentage((delta / 1000.0 / minutes).min(100.0))
}

fn spend_per_hour(
    current_cost: f64,
    previous_cost: f64,
    elapsed_millis: u128,
    previous: &SessionState,
) -> f32 {
    if !previous.initialized || elapsed_millis == 0 || current_cost <= previous_cost {
        return 0.0;
    }
    let hours = elapsed_to_units(elapsed_millis, MILLIS_PER_HOUR);
    if hours <= 0.0 {
        return 0.0;
    }
    crate::numeric::clamp_percentage(((current_cost - previous_cost) / hours).min(100.0))
}

fn elapsed_to_units(elapsed_millis: u128, millis_per_unit: f64) -> f64 {
    let capped = u64::try_from(elapsed_millis).unwrap_or(u64::MAX);
    wide_float_from_tokens(capped) / millis_per_unit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload_with(usage_json: &str) -> StatusLinePayload {
        StatusLinePayload::parse_lenient(usage_json)
    }

    #[test]
    fn cache_hit_ratio_is_reads_over_all_input() {
        let payload = payload_with(
            r#"{"context_window":{"current_usage":{"input_tokens":1000,
                "cache_creation_input_tokens":1000,"cache_read_input_tokens":8000}}}"#,
        );
        let metrics = DerivedMetrics::from_payload(&payload, &SessionState::default());
        assert!((metrics.cache_hit_ratio - 0.8).abs() < 0.001);
    }

    #[test]
    fn cache_hit_ratio_is_zero_when_no_usage_is_reported() {
        let metrics = DerivedMetrics::from_payload(&payload_with("{}"), &SessionState::default());
        assert!(metrics.cache_hit_ratio.abs() < f32::EPSILON);
    }

    #[test]
    fn burn_rate_stays_at_the_previous_value_on_the_first_sample() {
        let payload = payload_with(r#"{"context_window":{"total_input_tokens":50000}}"#);
        let metrics = DerivedMetrics::from_payload(&payload, &SessionState::default());
        assert!(metrics.thousand_tokens_per_minute.abs() < f32::EPSILON);
    }

    #[test]
    fn compaction_estimate_reaches_zero_past_the_threshold() {
        let payload = payload_with(
            r#"{"context_window":{"total_input_tokens":190000,"context_window_size":200000}}"#,
        );
        let metrics = DerivedMetrics::from_payload(&payload, &SessionState::default());
        assert_eq!(metrics.minutes_until_compaction(92.0), 0);
    }

    #[test]
    fn compaction_estimate_is_never_zero_before_the_threshold() {
        let payload = payload_with(
            r#"{"context_window":{"total_input_tokens":180000,"context_window_size":200000}}"#,
        );
        let metrics = DerivedMetrics::from_payload(&payload, &SessionState::default());
        assert_eq!(metrics.minutes_until_compaction(92.0), 1);
    }

    #[test]
    fn absent_rate_limits_are_reported_as_missing() {
        let metrics = DerivedMetrics::from_payload(&payload_with("{}"), &SessionState::default());
        assert!(!metrics.has_rate_limit_data);
        assert!(metrics.five_hour_percentage.abs() < f32::EPSILON);
    }
}
