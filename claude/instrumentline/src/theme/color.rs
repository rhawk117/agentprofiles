use crate::numeric::{channel_from_float, count_from_float, interpolate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Rgb {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);

    #[must_use]
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    #[must_use]
    pub fn blended_with(self, other: Self, amount: f32) -> Self {
        let ratio = amount.clamp(0.0, 1.0);
        Self::new(
            channel_from_float(interpolate(
                f32::from(self.red),
                f32::from(other.red),
                ratio,
            )),
            channel_from_float(interpolate(
                f32::from(self.green),
                f32::from(other.green),
                ratio,
            )),
            channel_from_float(interpolate(
                f32::from(self.blue),
                f32::from(other.blue),
                ratio,
            )),
        )
    }

    #[must_use]
    pub fn from_hue_saturation_lightness(hue: f32, saturation: f32, lightness: f32) -> Self {
        let hue_wrapped = hue.rem_euclid(360.0);
        let saturation_unit = saturation.clamp(0.0, 100.0) / 100.0;
        let lightness_unit = lightness.clamp(0.0, 100.0) / 100.0;

        let chroma = (1.0 - lightness_unit.mul_add(2.0, -1.0).abs()) * saturation_unit;
        let sector = hue_wrapped / 60.0;
        let secondary = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
        let offset = chroma.mul_add(-0.5, lightness_unit);

        let (red_unit, green_unit, blue_unit) = match count_from_float(sector, 5) {
            0 => (chroma, secondary, 0.0),
            1 => (secondary, chroma, 0.0),
            2 => (0.0, chroma, secondary),
            3 => (0.0, secondary, chroma),
            4 => (secondary, 0.0, chroma),
            _ => (chroma, 0.0, secondary),
        };

        Self::new(
            channel_from_float((red_unit + offset) * 255.0),
            channel_from_float((green_unit + offset) * 255.0),
            channel_from_float((blue_unit + offset) * 255.0),
        )
    }

    #[must_use]
    pub fn relative_luminance(self) -> f32 {
        f32::from(self.blue).mul_add(
            0.0722,
            f32::from(self.red).mul_add(0.2126, f32::from(self.green) * 0.7152),
        ) / 255.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blending_at_the_endpoints_returns_the_endpoints() {
        let start = Rgb::new(0, 0, 0);
        let end = Rgb::new(200, 100, 50);
        assert_eq!(start.blended_with(end, 0.0), start);
        assert_eq!(start.blended_with(end, 1.0), end);
    }

    #[test]
    fn blending_is_clamped_outside_the_unit_range() {
        let start = Rgb::new(0, 0, 0);
        let end = Rgb::new(200, 100, 50);
        assert_eq!(start.blended_with(end, 4.0), end);
        assert_eq!(start.blended_with(end, -4.0), start);
    }

    #[test]
    fn hue_zero_full_saturation_is_red() {
        assert_eq!(
            Rgb::from_hue_saturation_lightness(0.0, 100.0, 50.0),
            Rgb::new(255, 0, 0)
        );
    }

    #[test]
    fn hue_one_twenty_full_saturation_is_green() {
        assert_eq!(
            Rgb::from_hue_saturation_lightness(120.0, 100.0, 50.0),
            Rgb::new(0, 255, 0)
        );
    }

    #[test]
    fn zero_saturation_is_grey_at_any_hue() {
        let grey = Rgb::from_hue_saturation_lightness(210.0, 0.0, 50.0);
        assert_eq!(grey.red, grey.green);
        assert_eq!(grey.green, grey.blue);
    }

    #[test]
    fn negative_hue_wraps_into_range() {
        assert_eq!(
            Rgb::from_hue_saturation_lightness(-120.0, 100.0, 50.0),
            Rgb::from_hue_saturation_lightness(240.0, 100.0, 50.0)
        );
    }
}
