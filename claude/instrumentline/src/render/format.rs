use crate::numeric::wide_float_from_tokens;

#[must_use]
pub fn compact_count(value: u64) -> String {
    if value < 1_000 {
        return value.to_string();
    }
    if value < 10_000 {
        let thousands = wide_float_from_tokens(value) / 1_000.0;
        return format!("{thousands:.1}k");
    }
    if value < 1_000_000 {
        return format!("{}k", value / 1_000);
    }
    let millions = wide_float_from_tokens(value) / 1_000_000.0;
    if (millions - millions.round()).abs() < 0.05 {
        format!("{}m", millions.round())
    } else {
        format!("{millions:.1}m")
    }
}

#[must_use]
pub fn window_label(window_tokens: u64) -> String {
    if window_tokens >= 1_000_000 {
        let millions = wide_float_from_tokens(window_tokens) / 1_000_000.0;
        if (millions - millions.round()).abs() < 0.05 {
            return format!("{}m", millions.round());
        }
        return format!("{millions:.1}m");
    }
    format!("{}k", window_tokens / 1_000)
}

#[must_use]
pub fn duration_from_millis(total_millis: u64) -> String {
    let total_seconds = total_millis / 1_000;
    let hours = total_seconds / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else {
        format!("{minutes}m{seconds:02}s")
    }
}

#[must_use]
pub fn countdown_to_epoch(reset_epoch_seconds: u64, now_epoch_seconds: u64) -> String {
    let remaining = reset_epoch_seconds.saturating_sub(now_epoch_seconds);
    if remaining == 0 {
        return "now".to_owned();
    }
    let days = remaining / 86_400;
    let hours = (remaining % 86_400) / 3_600;
    let minutes = (remaining % 3_600) / 60;
    if days > 0 {
        format!("{days}d{hours:02}h")
    } else if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}

#[must_use]
pub fn shorten_path_tail(path: &str) -> String {
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .unwrap_or(path)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_counts_render_verbatim() {
        assert_eq!(compact_count(0), "0");
        assert_eq!(compact_count(999), "999");
    }

    #[test]
    fn thousands_gain_one_decimal_until_ten_thousand() {
        assert_eq!(compact_count(1_800), "1.8k");
        assert_eq!(compact_count(9_900), "9.9k");
        assert_eq!(compact_count(12_400), "12k");
        assert_eq!(compact_count(470_000), "470k");
    }

    #[test]
    fn millions_drop_the_decimal_when_it_is_whole() {
        assert_eq!(compact_count(1_000_000), "1m");
        assert_eq!(compact_count(1_280_000), "1.3m");
    }

    #[test]
    fn window_labels_match_the_model_tier_wording() {
        assert_eq!(window_label(200_000), "200k");
        assert_eq!(window_label(1_000_000), "1m");
    }

    #[test]
    fn durations_switch_to_hours_past_sixty_minutes() {
        assert_eq!(duration_from_millis(65_000), "1m05s");
        assert_eq!(duration_from_millis(3_900_000), "1h05m");
    }

    #[test]
    fn countdowns_saturate_at_zero() {
        assert_eq!(countdown_to_epoch(100, 500), "now");
        assert_eq!(countdown_to_epoch(500, 100), "6m");
        assert_eq!(countdown_to_epoch(11_000, 100), "3h01m");
        assert_eq!(countdown_to_epoch(360_000, 0), "4d04h");
        assert_eq!(countdown_to_epoch(604_800, 0), "7d00h");
    }

    #[test]
    fn path_tail_ignores_trailing_separators() {
        assert_eq!(shorten_path_tail("/home/r/reconflux"), "reconflux");
        assert_eq!(shorten_path_tail("/home/r/reconflux/"), "reconflux");
        assert_eq!(shorten_path_tail("reconflux"), "reconflux");
        assert_eq!(shorten_path_tail(""), "");
    }
}
