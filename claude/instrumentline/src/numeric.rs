#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "layout counts stay far below the 24-bit exact range of f32"
)]
pub const fn float_from_count(count: usize) -> f32 {
    count as f32
}

#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "token counts lose no meaningful precision at display resolution"
)]
pub const fn float_from_tokens(tokens: u64) -> f32 {
    tokens as f32
}

#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is clamped into the target range before conversion"
)]
pub fn count_from_float(value: f32, maximum: usize) -> usize {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    let ceiling = float_from_count(maximum);
    let clamped = value.min(ceiling);
    (clamped as usize).min(maximum)
}

#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is clamped into u8 range before conversion"
)]
pub fn channel_from_float(value: f32) -> u8 {
    if !value.is_finite() {
        return 0;
    }
    value.clamp(0.0, 255.0).round() as u8
}

#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "percentages are clamped to 0..=100 before conversion"
)]
pub fn percent_from_float(value: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    value.clamp(0.0, 100.0).round() as u32
}

#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "token totals are clamped to a non negative finite range"
)]
pub fn tokens_from_float(value: f32) -> u64 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    value.min(float_from_tokens(u64::MAX / 2)).round() as u64
}

#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "token totals never approach the 53-bit exact range of f64"
)]
pub const fn wide_float_from_tokens(tokens: u64) -> f64 {
    tokens as f64
}

#[must_use]
#[expect(
    clippy::cast_possible_truncation,
    reason = "percentages are display values and are clamped before conversion"
)]
pub const fn clamp_percentage(value: f64) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 100.0) as f32
    } else {
        0.0
    }
}

#[must_use]
pub fn saturating_ratio(numerator: f32, denominator: f32) -> f32 {
    if denominator.abs() < f32::EPSILON || !denominator.is_finite() {
        0.0
    } else {
        (numerator / denominator).clamp(0.0, 1.0)
    }
}

#[must_use]
pub fn normalize_between(value: f32, low: f32, high: f32) -> f32 {
    if (high - low).abs() < f32::EPSILON {
        return 0.0;
    }
    ((value - low) / (high - low)).clamp(0.0, 1.0)
}

#[must_use]
pub fn interpolate(from: f32, to: f32, amount: f32) -> f32 {
    to.mul_add(amount, from * (1.0 - amount))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_from_float_clamps_to_maximum() {
        assert_eq!(count_from_float(99.0, 10), 10);
        assert_eq!(count_from_float(-4.0, 10), 0);
        assert_eq!(count_from_float(f32::NAN, 10), 0);
        assert_eq!(count_from_float(3.9, 10), 3);
    }

    #[test]
    fn channel_from_float_rounds_and_clamps() {
        assert_eq!(channel_from_float(-12.0), 0);
        assert_eq!(channel_from_float(300.0), 255);
        assert_eq!(channel_from_float(127.6), 128);
    }

    #[test]
    fn saturating_ratio_guards_zero_denominator() {
        assert!((saturating_ratio(5.0, 0.0)).abs() < f32::EPSILON);
        assert!((saturating_ratio(5.0, 10.0) - 0.5).abs() < f32::EPSILON);
        assert!((saturating_ratio(50.0, 10.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn normalize_between_clamps_both_ends() {
        assert!((normalize_between(0.0, 10.0, 20.0)).abs() < f32::EPSILON);
        assert!((normalize_between(30.0, 10.0, 20.0) - 1.0).abs() < f32::EPSILON);
        assert!((normalize_between(15.0, 10.0, 20.0) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn interpolate_moves_toward_target() {
        assert!((interpolate(0.0, 10.0, 0.5) - 5.0).abs() < f32::EPSILON);
        assert!((interpolate(10.0, 10.0, 0.5) - 10.0).abs() < f32::EPSILON);
    }
}
