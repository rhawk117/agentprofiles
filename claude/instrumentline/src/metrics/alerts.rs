use crate::config::Configuration;
use crate::metrics::derived::DerivedMetrics;
use crate::numeric::percent_from_float;
use crate::state::{EasedValues, SessionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    Notice,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub severity: AlertSeverity,
    pub label: String,
    pub detail: String,
}

impl Alert {
    fn new(severity: AlertSeverity, label: &str, detail: String) -> Self {
        Self {
            severity,
            label: label.to_owned(),
            detail,
        }
    }
}

#[must_use]
pub fn evaluate_alerts(
    metrics: &DerivedMetrics,
    eased: &EasedValues,
    state: &SessionState,
    configuration: &Configuration,
) -> Vec<Alert> {
    let thresholds = &configuration.alert_thresholds;
    let mut alerts = Vec::new();

    append_cache_write_alert(&mut alerts, state);
    append_cache_hit_alert(&mut alerts, eased, configuration);
    append_burn_alert(&mut alerts, eased, configuration);
    append_long_session_alert(&mut alerts, metrics, configuration);
    append_context_alert(&mut alerts, metrics, eased, configuration);
    append_window_alert(&mut alerts, "5H", eased.five_hour_percentage, configuration);
    if eased.seven_day_percentage >= thresholds.window_notice_percentage {
        append_window_alert(&mut alerts, "7D", eased.seven_day_percentage, configuration);
    }

    alerts.sort_by_key(|alert| std::cmp::Reverse(alert.severity));
    alerts
}

fn append_cache_write_alert(alerts: &mut Vec<Alert>, state: &SessionState) {
    match state.consecutive_cache_write_turns {
        0 | 1 => {}
        2 => alerts.push(Alert::new(
            AlertSeverity::Warning,
            "CACHE WRITE",
            "re-write detected".to_owned(),
        )),
        turns => alerts.push(Alert::new(
            AlertSeverity::Critical,
            "CACHE CHURN",
            format!("writes x{turns} turns - billed at 1.25x"),
        )),
    }
}

fn append_cache_hit_alert(
    alerts: &mut Vec<Alert>,
    eased: &EasedValues,
    configuration: &Configuration,
) {
    let thresholds = &configuration.alert_thresholds;
    if eased.cache_hit_ratio >= thresholds.cache_hit_warning {
        return;
    }
    let percentage = percent_from_float(eased.cache_hit_ratio * 100.0);
    let severity = if eased.cache_hit_ratio < thresholds.cache_hit_critical {
        AlertSeverity::Critical
    } else {
        AlertSeverity::Warning
    };
    alerts.push(Alert::new(
        severity,
        "LOW HIT",
        format!("{percentage}% cached - context shifting"),
    ));
}

fn append_burn_alert(alerts: &mut Vec<Alert>, eased: &EasedValues, configuration: &Configuration) {
    if eased.thousand_tokens_per_minute <= configuration.alert_thresholds.burn_thousands_per_minute
    {
        return;
    }
    let dollars_per_hour =
        eased.thousand_tokens_per_minute * configuration.dollars_per_thousand_tokens;
    alerts.push(Alert::new(
        AlertSeverity::Warning,
        "BURN",
        format!("${dollars_per_hour:.2}/hr at this pace"),
    ));
}

fn append_long_session_alert(
    alerts: &mut Vec<Alert>,
    metrics: &DerivedMetrics,
    configuration: &Configuration,
) {
    if metrics.session_minutes < configuration.alert_thresholds.long_session_minutes {
        return;
    }
    alerts.push(Alert::new(
        AlertSeverity::Notice,
        "LONG SESSION",
        "cache TTL churn outpacing savings".to_owned(),
    ));
}

fn append_context_alert(
    alerts: &mut Vec<Alert>,
    metrics: &DerivedMetrics,
    eased: &EasedValues,
    configuration: &Configuration,
) {
    let near = configuration.compaction_threshold_percentage - 6.0;
    if configuration.auto_compact_enabled && eased.context_percentage >= near {
        let minutes =
            metrics.minutes_until_compaction(configuration.compaction_threshold_percentage);
        alerts.push(Alert::new(
            AlertSeverity::Critical,
            "COMPACT",
            format!("auto-compact in ~{minutes}m"),
        ));
    } else if eased.context_percentage >= configuration.alert_thresholds.context_notice_percentage {
        let percentage = percent_from_float(eased.context_percentage);
        alerts.push(Alert::new(
            AlertSeverity::Warning,
            "CTX",
            format!("{percentage}% of window consumed"),
        ));
    }
}

fn append_window_alert(
    alerts: &mut Vec<Alert>,
    label: &str,
    percentage: f32,
    configuration: &Configuration,
) {
    let thresholds = &configuration.alert_thresholds;
    let rounded = percent_from_float(percentage);
    if percentage >= thresholds.window_critical_percentage {
        let minutes_left = percent_from_float((100.0 - percentage) * 7.0).max(1);
        alerts.push(Alert::new(
            AlertSeverity::Critical,
            label,
            format!("{rounded}% used - ~{minutes_left}m of headroom"),
        ));
    } else if percentage >= thresholds.window_notice_percentage {
        alerts.push(Alert::new(
            AlertSeverity::Warning,
            label,
            format!("{rounded}% used"),
        ));
    }
}

#[must_use]
pub fn rotating_alert(alerts: &[Alert], frame: u64, rotation_frames: u64) -> Option<&Alert> {
    if alerts.is_empty() {
        return None;
    }
    let period = rotation_frames.max(1);
    let index = usize::try_from((frame / period) % alerts.len() as u64).unwrap_or(0);
    alerts.get(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::StatusLinePayload;

    fn metrics_for(json: &str) -> DerivedMetrics {
        DerivedMetrics::from_payload(
            &StatusLinePayload::parse_lenient(json),
            &SessionState::default(),
        )
    }

    fn calm_values() -> EasedValues {
        EasedValues {
            context_percentage: 20.0,
            five_hour_percentage: 10.0,
            seven_day_percentage: 5.0,
            cache_hit_ratio: 0.95,
            thousand_tokens_per_minute: 1.0,
        }
    }

    #[test]
    fn a_calm_session_raises_nothing() {
        let alerts = evaluate_alerts(
            &metrics_for("{}"),
            &calm_values(),
            &SessionState::default(),
            &Configuration::default(),
        );
        assert!(alerts.is_empty(), "unexpected alerts: {alerts:?}");
    }

    #[test]
    fn sustained_cache_writes_escalate_to_critical() {
        let mut state = SessionState::default();
        for tokens in [1000, 2000, 3000] {
            state.record_cache_write_turn(tokens);
        }
        let alerts = evaluate_alerts(
            &metrics_for("{}"),
            &calm_values(),
            &state,
            &Configuration::default(),
        );
        assert_eq!(
            alerts.first().map(|alert| alert.severity),
            Some(AlertSeverity::Critical)
        );
        assert_eq!(
            alerts.first().map(|alert| alert.label.as_str()),
            Some("CACHE CHURN")
        );
    }

    #[test]
    fn a_collapsed_hit_ratio_is_critical_and_a_soft_one_is_a_warning() {
        let configuration = Configuration::default();
        let mut values = calm_values();
        values.cache_hit_ratio = 0.30;
        let critical = evaluate_alerts(
            &metrics_for("{}"),
            &values,
            &SessionState::default(),
            &configuration,
        );
        assert!(
            critical
                .iter()
                .any(|alert| alert.severity == AlertSeverity::Critical)
        );

        values.cache_hit_ratio = 0.55;
        let warning = evaluate_alerts(
            &metrics_for("{}"),
            &values,
            &SessionState::default(),
            &configuration,
        );
        assert!(
            warning
                .iter()
                .any(|alert| alert.label == "LOW HIT" && alert.severity == AlertSeverity::Warning)
        );
    }

    #[test]
    fn alerts_are_ordered_most_severe_first() {
        let mut values = calm_values();
        values.cache_hit_ratio = 0.25;
        values.five_hour_percentage = 80.0;
        values.thousand_tokens_per_minute = 20.0;
        let alerts = evaluate_alerts(
            &metrics_for("{}"),
            &values,
            &SessionState::default(),
            &Configuration::default(),
        );
        let severities: Vec<_> = alerts.iter().map(|alert| alert.severity).collect();
        let mut sorted = severities.clone();
        sorted.sort_by(|left, right| right.cmp(left));
        assert_eq!(severities, sorted);
    }

    #[test]
    fn the_seven_day_window_stays_quiet_until_it_matters() {
        let mut values = calm_values();
        values.seven_day_percentage = 40.0;
        let quiet = evaluate_alerts(
            &metrics_for("{}"),
            &values,
            &SessionState::default(),
            &Configuration::default(),
        );
        assert!(quiet.iter().all(|alert| alert.label != "7D"));

        values.seven_day_percentage = 92.0;
        let loud = evaluate_alerts(
            &metrics_for("{}"),
            &values,
            &SessionState::default(),
            &Configuration::default(),
        );
        assert!(loud.iter().any(|alert| alert.label == "7D"));
    }

    #[test]
    fn disabling_auto_compact_downgrades_the_context_alert() {
        let configuration = Configuration {
            auto_compact_enabled: false,
            ..Configuration::default()
        };
        let mut values = calm_values();
        values.context_percentage = 89.0;
        let alerts = evaluate_alerts(
            &metrics_for("{}"),
            &values,
            &SessionState::default(),
            &configuration,
        );
        assert!(alerts.iter().all(|alert| alert.label != "COMPACT"));
        assert!(alerts.iter().any(|alert| alert.label == "CTX"));
    }

    #[test]
    fn rotation_walks_every_alert_and_wraps() {
        let alerts = vec![
            Alert::new(AlertSeverity::Critical, "A", String::new()),
            Alert::new(AlertSeverity::Warning, "B", String::new()),
        ];
        assert_eq!(
            rotating_alert(&alerts, 0, 5).map(|a| a.label.as_str()),
            Some("A")
        );
        assert_eq!(
            rotating_alert(&alerts, 5, 5).map(|a| a.label.as_str()),
            Some("B")
        );
        assert_eq!(
            rotating_alert(&alerts, 10, 5).map(|a| a.label.as_str()),
            Some("A")
        );
        assert_eq!(rotating_alert(&[], 0, 5), None);
    }
}
