use crate::theme::color::Rgb;

pub const COOL_HUE: f32 = 168.0;
pub const HOT_HUE: f32 = 6.0;
const HUE_RESPONSE_EXPONENT: f32 = 0.62;

#[derive(Debug, Clone, Copy)]
pub struct TemperaturePalette {
    health: f32,
}

impl TemperaturePalette {
    #[must_use]
    pub const fn from_health(health: f32) -> Self {
        Self {
            health: health.clamp(0.0, 1.0),
        }
    }

    #[must_use]
    pub const fn health(self) -> f32 {
        self.health
    }

    #[must_use]
    pub fn hue(self) -> f32 {
        (HOT_HUE - COOL_HUE).mul_add(self.health.powf(HUE_RESPONSE_EXPONENT), COOL_HUE)
    }

    #[must_use]
    pub fn primary(self) -> Rgb {
        Rgb::from_hue_saturation_lightness(self.hue(), self.health.mul_add(22.0, 70.0), 58.0)
    }

    #[must_use]
    pub fn bright(self) -> Rgb {
        Rgb::from_hue_saturation_lightness(self.hue(), self.health.mul_add(14.0, 82.0), 66.0)
    }

    #[must_use]
    pub fn shadow(self) -> Rgb {
        Rgb::from_hue_saturation_lightness(self.hue(), self.health.mul_add(10.0, 62.0), 24.0)
    }

    #[must_use]
    pub fn readout(self) -> Rgb {
        NEUTRAL_READOUT.blended_with(self.bright(), (self.health * 1.3).clamp(0.0, 1.0))
    }

    #[must_use]
    pub fn is_alarming(self) -> bool {
        self.health >= 0.85
    }
}

pub const NEUTRAL_READOUT: Rgb = Rgb::new(0xd8, 0xe6, 0xea);
pub const NEUTRAL_LABEL: Rgb = Rgb::new(0x4b, 0x64, 0x70);
pub const NEUTRAL_TRACK: Rgb = Rgb::new(0x1e, 0x2c, 0x33);
pub const NEUTRAL_FRAME: Rgb = Rgb::new(0x1c, 0x2b, 0x30);
pub const NEUTRAL_MUTED: Rgb = Rgb::new(0x3c, 0x50, 0x58);
pub const ACCENT_CYAN: Rgb = Rgb::new(0x22, 0xd3, 0xee);
pub const ACCENT_GREEN: Rgb = Rgb::new(0x34, 0xd3, 0x99);
pub const ACCENT_AMBER: Rgb = Rgb::new(0xff, 0xb0, 0x00);
pub const ACCENT_ORANGE: Rgb = Rgb::new(0xff, 0x8a, 0x5c);
pub const ACCENT_RED: Rgb = Rgb::new(0xff, 0x3b, 0x30);
pub const ACCENT_VIOLET: Rgb = Rgb::new(0xc0, 0x84, 0xfc);
pub const ACCENT_MAGENTA: Rgb = Rgb::new(0xf4, 0x72, 0xb6);
pub const ACCENT_SLATE: Rgb = Rgb::new(0x6b, 0x7f, 0x8a);
pub const ACCENT_SKY: Rgb = Rgb::new(0x8f, 0xb8, 0xe8);
pub const ACCENT_MINT: Rgb = Rgb::new(0xa7, 0xf3, 0xd0);
pub const ACCENT_SAGE: Rgb = Rgb::new(0x7f, 0x9a, 0x92);
pub const ACCENT_SPEND: Rgb = Rgb::new(0x7e, 0xe0, 0x8a);
pub const DANGER_TRACK: Rgb = Rgb::new(0x4a, 0x22, 0x22);
pub const ALARM_FLASH: Rgb = Rgb::new(0xff, 0xe0, 0xe0);

#[must_use]
pub const fn effort_color(level: &str) -> Rgb {
    match level.as_bytes() {
        b"low" => ACCENT_SLATE,
        b"medium" => ACCENT_GREEN,
        b"xhigh" => ACCENT_ORANGE,
        b"max" => ACCENT_RED,
        _ => ACCENT_VIOLET,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hue_travels_from_cool_to_hot_monotonically() {
        let mut previous = f32::MAX;
        for step in 0..=10 {
            #[expect(clippy::cast_precision_loss, reason = "loop bound is ten")]
            let health = step as f32 / 10.0;
            let hue = TemperaturePalette::from_health(health).hue();
            assert!(hue <= previous, "hue must not increase as health worsens");
            previous = hue;
        }
    }

    #[test]
    fn endpoints_land_on_the_declared_hues() {
        assert!((TemperaturePalette::from_health(0.0).hue() - COOL_HUE).abs() < 0.01);
        assert!((TemperaturePalette::from_health(1.0).hue() - HOT_HUE).abs() < 0.01);
    }

    #[test]
    fn health_is_clamped_on_construction() {
        assert!((TemperaturePalette::from_health(4.0).health() - 1.0).abs() < f32::EPSILON);
        assert!(TemperaturePalette::from_health(-4.0).health().abs() < f32::EPSILON);
    }

    #[test]
    fn alarming_only_at_the_top_of_the_range() {
        assert!(!TemperaturePalette::from_health(0.84).is_alarming());
        assert!(TemperaturePalette::from_health(0.86).is_alarming());
    }

    #[test]
    fn effort_levels_each_have_a_distinct_color() {
        let colors = ["low", "medium", "high", "xhigh", "max"].map(effort_color);
        for (index, color) in colors.iter().enumerate() {
            for other in colors.iter().skip(index + 1) {
                assert_ne!(color, other);
            }
        }
    }

    #[test]
    fn unknown_effort_falls_back_to_the_high_color() {
        assert_eq!(effort_color("unheard-of"), effort_color("high"));
    }
}
