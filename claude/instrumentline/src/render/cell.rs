use std::fmt::Write as _;

use crate::theme::color::Rgb;

pub const RESET_SEQUENCE: &str = "\u{1b}[0m";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

impl Style {
    #[must_use]
    pub const fn plain() -> Self {
        Self {
            foreground: None,
            background: None,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    #[must_use]
    pub const fn foreground(color: Rgb) -> Self {
        Self {
            foreground: Some(color),
            ..Self::plain()
        }
    }

    #[must_use]
    pub const fn inverted(foreground: Rgb, background: Rgb) -> Self {
        Self {
            foreground: Some(foreground),
            background: Some(background),
            bold: true,
            italic: false,
            underline: false,
        }
    }

    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    #[must_use]
    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    #[must_use]
    pub const fn is_plain(&self) -> bool {
        self.foreground.is_none()
            && self.background.is_none()
            && !self.bold
            && !self.italic
            && !self.underline
    }

    fn write_prefix_into(&self, buffer: &mut String) {
        if self.is_plain() {
            return;
        }
        buffer.push_str("\u{1b}[");
        let mut needs_separator = false;
        let mut push_code = |code: &str, buffer: &mut String| {
            if needs_separator {
                buffer.push(';');
            }
            buffer.push_str(code);
            needs_separator = true;
        };
        if self.bold {
            push_code("1", buffer);
        }
        if self.italic {
            push_code("3", buffer);
        }
        if self.underline {
            push_code("4", buffer);
        }
        if let Some(color) = self.foreground {
            push_code("38;2", buffer);
            let _ = write!(buffer, ";{};{};{}", color.red, color.green, color.blue);
        }
        if let Some(color) = self.background {
            push_code("48;2", buffer);
            let _ = write!(buffer, ";{};{};{}", color.red, color.green, color.blue);
        }
        buffer.push('m');
    }
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub text: String,
    pub style: Style,
}

impl Segment {
    #[must_use]
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Self {
            text: text.into(),
            style,
        }
    }

    #[must_use]
    pub fn plain(text: impl Into<String>) -> Self {
        Self::new(text, Style::plain())
    }

    #[must_use]
    pub fn spacer(width: usize) -> Self {
        Self::plain(" ".repeat(width))
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.text.chars().count()
    }

    #[must_use]
    pub fn truncated_to(&self, columns: usize) -> Self {
        Self::new(
            self.text.chars().take(columns).collect::<String>(),
            self.style,
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct Line {
    segments: Vec<Segment>,
}

impl Line {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    #[must_use]
    pub const fn from_segments(segments: Vec<Segment>) -> Self {
        Self { segments }
    }

    pub fn push(&mut self, segment: Segment) {
        self.segments.push(segment);
    }

    pub fn extend(&mut self, segments: impl IntoIterator<Item = Segment>) {
        self.segments.extend(segments);
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.segments.iter().map(Segment::width).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    #[must_use]
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    #[must_use]
    pub fn truncated_to(&self, columns: usize) -> Self {
        let mut remaining = columns;
        let mut kept = Vec::new();
        for segment in &self.segments {
            if remaining == 0 {
                break;
            }
            let width = segment.width();
            if width <= remaining {
                kept.push(segment.clone());
                remaining -= width;
            } else {
                kept.push(segment.truncated_to(remaining));
                remaining = 0;
            }
        }
        Self { segments: kept }
    }

    #[must_use]
    pub fn to_ansi(&self) -> String {
        let mut buffer = String::with_capacity(self.width() * 4);
        for segment in &self.segments {
            if segment.text.is_empty() {
                continue;
            }
            if segment.style.is_plain() {
                buffer.push_str(&segment.text);
            } else {
                segment.style.write_prefix_into(&mut buffer);
                buffer.push_str(&segment.text);
                buffer.push_str(RESET_SEQUENCE);
            }
        }
        buffer
    }

    #[must_use]
    pub fn to_plain_text(&self) -> String {
        self.segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_segments_emit_no_escape_codes() {
        let line = Line::from_segments(vec![Segment::plain("hello")]);
        assert_eq!(line.to_ansi(), "hello");
    }

    #[test]
    fn foreground_colour_emits_a_truecolor_sequence() {
        let line = Line::from_segments(vec![Segment::new(
            "x",
            Style::foreground(Rgb::new(0x22, 0xd3, 0xee)),
        )]);
        assert_eq!(line.to_ansi(), "\u{1b}[38;2;34;211;238mx\u{1b}[0m");
    }

    #[test]
    fn attributes_are_emitted_in_ascending_code_order() {
        let style = Style::foreground(Rgb::new(1, 2, 3))
            .bold()
            .italic()
            .underline();
        let line = Line::from_segments(vec![Segment::new("x", style)]);
        assert_eq!(line.to_ansi(), "\u{1b}[1;3;4;38;2;1;2;3mx\u{1b}[0m");
    }

    #[test]
    fn every_styled_segment_resets_so_style_never_bleeds() {
        let line = Line::from_segments(vec![
            Segment::new("a", Style::foreground(Rgb::WHITE)),
            Segment::plain("b"),
        ]);
        let rendered = line.to_ansi();
        assert!(rendered.ends_with("\u{1b}[0mb"));
    }

    #[test]
    fn width_counts_characters_not_bytes() {
        let line = Line::from_segments(vec![Segment::plain("━━━"), Segment::plain("ab")]);
        assert_eq!(line.width(), 5);
    }

    #[test]
    fn truncation_stops_exactly_at_the_column_budget() {
        let line = Line::from_segments(vec![Segment::plain("abcdef"), Segment::plain("ghijkl")]);
        let cut = line.truncated_to(8);
        assert_eq!(cut.width(), 8);
        assert_eq!(cut.to_plain_text(), "abcdefgh");
    }

    #[test]
    fn truncation_to_zero_yields_an_empty_line() {
        let line = Line::from_segments(vec![Segment::plain("abc")]);
        assert!(line.truncated_to(0).is_empty());
    }

    #[test]
    fn truncation_preserves_multibyte_glyph_boundaries() {
        let line = Line::from_segments(vec![Segment::plain("━━━━━")]);
        assert_eq!(line.truncated_to(2).to_plain_text(), "━━");
    }

    #[test]
    fn empty_segments_are_skipped_entirely() {
        let line = Line::from_segments(vec![
            Segment::new("", Style::foreground(Rgb::WHITE)),
            Segment::plain("z"),
        ]);
        assert_eq!(line.to_ansi(), "z");
    }
}
