use serde::{Deserialize, Serialize};

use crate::numeric::count_from_float;

pub const HORIZONTAL_EIGHTHS: [&str; 9] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
pub const VERTICAL_EIGHTHS: [&str; 9] = ["", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
pub const BRAILLE_SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Haiku,
    Sonnet,
    Fable,
    Opus,
    Unknown,
}

impl ModelTier {
    #[must_use]
    pub fn from_model_identifier(identifier: &str, display_name: &str) -> Self {
        let haystack = format!("{identifier} {display_name}").to_ascii_lowercase();
        if haystack.contains("haiku") {
            Self::Haiku
        } else if haystack.contains("sonnet") {
            Self::Sonnet
        } else if haystack.contains("fable") {
            Self::Fable
        } else if haystack.contains("opus") {
            Self::Opus
        } else {
            Self::Unknown
        }
    }

    #[must_use]
    pub const fn table_index(self) -> usize {
        match self {
            Self::Haiku => 0,
            Self::Sonnet => 1,
            Self::Fable => 2,
            Self::Opus | Self::Unknown => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    #[default]
    Default,
    Plan,
    AcceptEdits,
    Auto,
    DontAsk,
    BypassPermissions,
}

impl PermissionMode {
    #[must_use]
    pub fn from_hook_value(raw: &str) -> Self {
        match raw.trim() {
            "plan" => Self::Plan,
            "acceptEdits" => Self::AcceptEdits,
            "auto" => Self::Auto,
            "dontAsk" => Self::DontAsk,
            "bypassPermissions" => Self::BypassPermissions,
            _ => Self::Default,
        }
    }

    #[must_use]
    pub const fn table_index(self) -> usize {
        match self {
            Self::Default => 0,
            Self::Plan => 1,
            Self::AcceptEdits => 2,
            Self::Auto | Self::DontAsk => 3,
            Self::BypassPermissions => 4,
        }
    }

    #[must_use]
    pub const fn short_tag(self) -> &'static str {
        match self {
            Self::Default => "DFLT",
            Self::Plan => "PLAN",
            Self::AcceptEdits => "EDIT",
            Self::Auto => "AUTO",
            Self::DontAsk => "NASK",
            Self::BypassPermissions => "BYPS",
        }
    }

    #[must_use]
    pub const fn rail_glyph(self) -> &'static str {
        match self {
            Self::Default => "│",
            Self::Plan => "╎",
            Self::AcceptEdits | Self::Auto | Self::DontAsk => "┃",
            Self::BypassPermissions => "║",
        }
    }

    #[must_use]
    pub const fn breathes(self) -> bool {
        matches!(self, Self::Plan)
    }

    #[must_use]
    pub const fn blinks(self) -> bool {
        matches!(self, Self::BypassPermissions)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlyphTable {
    pub model_tiers: [String; 4],
    pub permission_modes: [String; 5],
    pub compaction_marker: String,
    pub tape_tick: String,
    pub tape_track: String,
    pub tape_filled: String,
    pub gauge_open: String,
    pub gauge_close: String,
    pub gauge_empty: String,
    pub tool_calls: String,
    pub turns: String,
    pub subagents: String,
    pub errors: String,
    pub branch: String,
    pub reset_clock: String,
    pub idle_marker: String,
    pub alert_notice: String,
    pub alert_warning: String,
    pub alert_critical: String,
    pub healthy: String,
}

impl Default for GlyphTable {
    fn default() -> Self {
        Self {
            model_tiers: ["⌁".into(), "❖".into(), "☾".into(), "⌬".into()],
            permission_modes: ["○".into(), "⌖".into(), "✎".into(), "⟳".into(), "⊘".into()],
            compaction_marker: "╏".into(),
            tape_tick: "┊".into(),
            tape_track: "╌".into(),
            tape_filled: "━".into(),
            gauge_open: "▕".into(),
            gauge_close: "▏".into(),
            gauge_empty: "·".into(),
            tool_calls: "⚒".into(),
            turns: "↻".into(),
            subagents: "⧉".into(),
            errors: "✗".into(),
            branch: "⎇".into(),
            reset_clock: "↻".into(),
            idle_marker: "·".into(),
            alert_notice: "◆".into(),
            alert_warning: "▲".into(),
            alert_critical: "⚠".into(),
            healthy: "✓".into(),
        }
    }
}

impl GlyphTable {
    #[must_use]
    pub fn ascii_fallback() -> Self {
        Self {
            model_tiers: ["-".into(), "=".into(), "+".into(), "*".into()],
            permission_modes: ["o".into(), "?".into(), "e".into(), "a".into(), "!".into()],
            compaction_marker: "|".into(),
            tape_tick: "'".into(),
            tape_track: "-".into(),
            tape_filled: "=".into(),
            gauge_open: "[".into(),
            gauge_close: "]".into(),
            gauge_empty: ".".into(),
            tool_calls: "t".into(),
            turns: "r".into(),
            subagents: "a".into(),
            errors: "x".into(),
            branch: "b".into(),
            reset_clock: "~".into(),
            idle_marker: ".".into(),
            alert_notice: "-".into(),
            alert_warning: "!".into(),
            alert_critical: "!!".into(),
            healthy: "ok".into(),
        }
    }

    #[must_use]
    pub fn model_glyph(&self, tier: ModelTier) -> &str {
        self.model_tiers
            .get(tier.table_index())
            .map_or("*", String::as_str)
    }

    #[must_use]
    pub fn permission_glyph(&self, mode: PermissionMode) -> &str {
        self.permission_modes
            .get(mode.table_index())
            .map_or("o", String::as_str)
    }
}

#[must_use]
pub fn horizontal_partial(fraction: f32) -> &'static str {
    let index = count_from_float(fraction.clamp(0.0, 1.0) * 8.0, 8);
    HORIZONTAL_EIGHTHS.get(index).copied().unwrap_or("")
}

#[must_use]
pub fn vertical_partial(fraction: f32) -> &'static str {
    let index = count_from_float(fraction.clamp(0.0, 1.0) * 8.0, 8);
    VERTICAL_EIGHTHS.get(index).copied().unwrap_or("")
}

#[must_use]
pub fn spinner_frame(frame: u64) -> &'static str {
    let index = usize::try_from(frame % 10).unwrap_or(0);
    BRAILLE_SPINNER.get(index).copied().unwrap_or("*")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_tier_detection_reads_identifier_or_display_name() {
        assert_eq!(
            ModelTier::from_model_identifier("claude-opus-4-8", ""),
            ModelTier::Opus
        );
        assert_eq!(
            ModelTier::from_model_identifier("", "Haiku 4.5"),
            ModelTier::Haiku
        );
        assert_eq!(
            ModelTier::from_model_identifier("claude-sonnet-4-8", ""),
            ModelTier::Sonnet
        );
        assert_eq!(
            ModelTier::from_model_identifier("fable-1-2", ""),
            ModelTier::Fable
        );
        assert_eq!(
            ModelTier::from_model_identifier("something-else", ""),
            ModelTier::Unknown
        );
    }

    #[test]
    fn permission_mode_maps_every_documented_value() {
        assert_eq!(
            PermissionMode::from_hook_value("plan"),
            PermissionMode::Plan
        );
        assert_eq!(
            PermissionMode::from_hook_value("acceptEdits"),
            PermissionMode::AcceptEdits
        );
        assert_eq!(
            PermissionMode::from_hook_value("auto"),
            PermissionMode::Auto
        );
        assert_eq!(
            PermissionMode::from_hook_value("dontAsk"),
            PermissionMode::DontAsk
        );
        assert_eq!(
            PermissionMode::from_hook_value("bypassPermissions"),
            PermissionMode::BypassPermissions
        );
        assert_eq!(
            PermissionMode::from_hook_value("manual"),
            PermissionMode::Default
        );
        assert_eq!(PermissionMode::from_hook_value(""), PermissionMode::Default);
    }

    #[test]
    fn every_mode_tag_is_the_same_width() {
        for mode in [
            PermissionMode::Default,
            PermissionMode::Plan,
            PermissionMode::AcceptEdits,
            PermissionMode::Auto,
            PermissionMode::DontAsk,
            PermissionMode::BypassPermissions,
        ] {
            assert_eq!(mode.short_tag().chars().count(), 4, "{mode:?}");
        }
    }

    #[test]
    fn partial_blocks_span_the_full_range() {
        assert_eq!(horizontal_partial(0.0), "");
        assert_eq!(horizontal_partial(1.0), "█");
        assert_eq!(vertical_partial(0.5), "▄");
        assert_eq!(vertical_partial(2.0), "█");
    }

    #[test]
    fn ascii_fallback_uses_only_single_byte_characters() {
        let table = GlyphTable::ascii_fallback();
        let serialized = serde_json::to_string(&table).unwrap_or_default();
        assert!(serialized.is_ascii());
    }
}
