use crate::config::Configuration;
use crate::numeric::normalize_between;
use crate::state::EasedValues;

pub const DEMON_FLOOR: f32 = 0.92;
pub const SEVERE_FLOOR: f32 = 0.66;

#[must_use]
pub fn score_session_health(eased: &EasedValues, configuration: &Configuration) -> f32 {
    let ramps = &configuration.health_ramps;
    let weights = &configuration.health_weights;

    let context_pressure = normalize_between(
        eased.context_percentage,
        ramps.context_from,
        ramps.context_to,
    );
    let window_pressure = normalize_between(
        eased.five_hour_percentage,
        ramps.five_hour_from,
        ramps.five_hour_to,
    );
    let cache_pressure = normalize_between(
        eased.cache_hit_ratio,
        ramps.cache_hit_healthy,
        ramps.cache_hit_poor,
    );
    let burn_pressure = normalize_between(
        eased.thousand_tokens_per_minute,
        ramps.burn_from_thousands_per_minute,
        ramps.burn_to_thousands_per_minute,
    );

    let blended = burn_pressure.mul_add(
        weights.burn_rate,
        cache_pressure.mul_add(
            weights.cache_hit_ratio,
            context_pressure.mul_add(weights.context, window_pressure * weights.five_hour_window),
        ),
    );

    apply_emergency_floors(blended, eased, configuration).clamp(0.0, 1.0)
}

fn apply_emergency_floors(blended: f32, eased: &EasedValues, configuration: &Configuration) -> f32 {
    let thresholds = &configuration.alert_thresholds;
    let critical_window = thresholds.window_critical_percentage;
    let near_compaction = configuration.compaction_threshold_percentage - 4.0;

    if eased.context_percentage >= near_compaction
        || eased.five_hour_percentage >= critical_window
        || eased.seven_day_percentage >= critical_window
    {
        return blended.max(DEMON_FLOOR);
    }
    if eased.context_percentage >= thresholds.context_notice_percentage + 3.0
        || eased.five_hour_percentage >= thresholds.window_notice_percentage + 3.0
        || eased.cache_hit_ratio < thresholds.cache_hit_critical
    {
        return blended.max(SEVERE_FLOOR);
    }
    blended
}

#[cfg(test)]
mod tests {
    use super::*;

    fn healthy_session() -> EasedValues {
        EasedValues {
            context_percentage: 12.0,
            five_hour_percentage: 5.0,
            seven_day_percentage: 3.0,
            cache_hit_ratio: 0.95,
            thousand_tokens_per_minute: 1.0,
        }
    }

    #[test]
    fn a_fresh_session_scores_near_zero() {
        let score = score_session_health(&healthy_session(), &Configuration::default());
        assert!(score < 0.05, "expected a calm score, got {score}");
    }

    #[test]
    fn every_dimension_pushes_the_score_upward() {
        let configuration = Configuration::default();
        let baseline = score_session_health(&healthy_session(), &configuration);
        for mutate in [
            |values: &mut EasedValues| values.context_percentage = 70.0,
            |values: &mut EasedValues| values.five_hour_percentage = 70.0,
            |values: &mut EasedValues| values.cache_hit_ratio = 0.5,
            |values: &mut EasedValues| values.thousand_tokens_per_minute = 12.0,
        ] {
            let mut values = healthy_session();
            mutate(&mut values);
            assert!(score_session_health(&values, &configuration) > baseline);
        }
    }

    #[test]
    fn near_compaction_pins_the_score_to_the_emergency_floor() {
        let configuration = Configuration::default();
        let values = EasedValues {
            context_percentage: 89.0,
            ..healthy_session()
        };
        assert!(score_session_health(&values, &configuration) >= DEMON_FLOOR);
    }

    #[test]
    fn an_exhausted_window_pins_the_score_even_with_low_context() {
        let configuration = Configuration::default();
        let values = EasedValues {
            five_hour_percentage: 95.0,
            ..healthy_session()
        };
        assert!(score_session_health(&values, &configuration) >= DEMON_FLOOR);
    }

    #[test]
    fn a_collapsed_cache_hit_ratio_reaches_the_severe_floor() {
        let configuration = Configuration::default();
        let values = EasedValues {
            cache_hit_ratio: 0.20,
            ..healthy_session()
        };
        assert!(score_session_health(&values, &configuration) >= SEVERE_FLOOR);
    }

    #[test]
    fn the_score_never_leaves_the_unit_range() {
        let configuration = Configuration::default();
        let values = EasedValues {
            context_percentage: 500.0,
            five_hour_percentage: 500.0,
            seven_day_percentage: 500.0,
            cache_hit_ratio: -4.0,
            thousand_tokens_per_minute: 900.0,
        };
        let score = score_session_health(&values, &configuration);
        assert!((0.0..=1.0).contains(&score));
    }
}
