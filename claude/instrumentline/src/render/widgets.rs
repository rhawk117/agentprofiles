use crate::config::{Configuration, WindowSlotStrategy};
use crate::metrics::alerts::{Alert, AlertSeverity, rotating_alert};
use crate::metrics::derived::DerivedMetrics;
use crate::numeric::{count_from_float, float_from_count, percent_from_float};
use crate::payload::StatusLinePayload;
use crate::render::cell::{Segment, Style};
use crate::render::format::{compact_count, countdown_to_epoch, shorten_path_tail, window_label};
use crate::state::EasedValues;
use crate::theme::color::Rgb;
use crate::theme::glyphs::{
    ModelTier, PermissionMode, horizontal_partial, spinner_frame, vertical_partial,
};
use crate::theme::palette::{
    ACCENT_AMBER, ACCENT_CYAN, ACCENT_GREEN, ACCENT_MAGENTA, ACCENT_MINT, ACCENT_ORANGE,
    ACCENT_RED, ACCENT_SAGE, ACCENT_SKY, ACCENT_SPEND, ALARM_FLASH, DANGER_TRACK, NEUTRAL_FRAME,
    NEUTRAL_LABEL, NEUTRAL_MUTED, NEUTRAL_READOUT, NEUTRAL_TRACK, TemperaturePalette, effort_color,
};
use crate::transcript::ActivityCounters;

const RAIL_ROWS: usize = 3;
const TANK_EIGHTHS: usize = RAIL_ROWS * 8;
const GAUGE_CELLS: usize = 10;
const SHIMMER_CELLS_PER_FRAME: f32 = 5.0;
const SHIMMER_SPREAD: f32 = 2.4;

#[derive(Debug)]
pub struct RenderContext<'a> {
    pub configuration: &'a Configuration,
    pub payload: &'a StatusLinePayload,
    pub metrics: &'a DerivedMetrics,
    pub eased: &'a EasedValues,
    pub counters: &'a ActivityCounters,
    pub alerts: &'a [Alert],
    pub palette: TemperaturePalette,
    pub mode: PermissionMode,
    pub tier: ModelTier,
    pub frame: u64,
    pub now_epoch_seconds: u64,
}

impl RenderContext<'_> {
    const fn blinking(&self) -> bool {
        self.frame % 2 == 1
    }

    fn shimmer_at(&self, index: usize, width: usize) -> f32 {
        if width == 0 {
            return 0.0;
        }
        let period = float_from_count(width + 10);
        #[expect(
            clippy::cast_precision_loss,
            reason = "frame counter is a display clock"
        )]
        let travelled = (self.frame as f32 * SHIMMER_CELLS_PER_FRAME) % period - 5.0;
        let distance = float_from_count(index) - travelled;
        (-(distance * distance) / (2.0 * SHIMMER_SPREAD * SHIMMER_SPREAD)).exp()
    }
}

#[must_use]
pub fn mode_rail(context: &RenderContext<'_>, row_index: usize) -> Vec<Segment> {
    let mode = context.mode;
    let mut rail_color = mode_color(mode);
    if mode.breathes() {
        #[expect(
            clippy::cast_precision_loss,
            reason = "frame counter is a display clock"
        )]
        let phase = (context.frame as f32 / 2.0).sin().mul_add(0.5, 0.5);
        rail_color = rail_color.blended_with(Rgb::new(0x0b, 0x3d, 0x47), phase * 0.5);
    }
    if mode.blinks() && !context.blinking() {
        rail_color = Rgb::new(0x5c, 0x1f, 0x1f);
    }

    let filled_eighths = count_from_float(
        context.eased.context_percentage / 100.0 * 40.0 * 0.6,
        TANK_EIGHTHS,
    );
    let rows_from_bottom = RAIL_ROWS.saturating_sub(1).saturating_sub(row_index);
    let level = filled_eighths.saturating_sub(rows_from_bottom * 8).min(8);

    let tank = if level == 0 {
        Segment::new(
            context.configuration.glyphs.idle_marker.clone(),
            Style::foreground(Rgb::new(0x20, 0x24, 0x2e)),
        )
    } else {
        let shimmer = context.shimmer_at(rows_from_bottom, RAIL_ROWS);
        let color = context
            .palette
            .primary()
            .blended_with(Rgb::WHITE, shimmer * 0.3);
        Segment::new(
            vertical_partial(float_from_count(level) / 8.0),
            Style::foreground(color),
        )
    };

    vec![
        Segment::new(mode.rail_glyph(), Style::foreground(rail_color).bold()),
        tank,
        Segment::plain(" "),
    ]
}

#[must_use]
pub fn mode_badge(context: &RenderContext<'_>) -> Vec<Segment> {
    let mode = context.mode;
    let glyph = context.configuration.glyphs.permission_glyph(mode);
    let text = format!(" {glyph} {} ", mode.short_tag());
    if mode == PermissionMode::Default {
        return vec![Segment::new(text, Style::foreground(NEUTRAL_MUTED))];
    }
    let background = if mode.blinks() && !context.blinking() {
        Rgb::new(0x7f, 0x1d, 0x1d)
    } else {
        mode_color(mode)
    };
    vec![Segment::new(
        text,
        Style::inverted(Rgb::new(0x0a, 0x0b, 0x0e), background),
    )]
}

#[must_use]
pub fn model_identity(context: &RenderContext<'_>) -> Vec<Segment> {
    let tier = context.tier;
    let glyph = context.configuration.glyphs.model_glyph(tier);
    let color = tier_color(tier);
    let slug = model_slug(context.payload);
    let effort = context.payload.effort_level();

    let mut segments = vec![
        Segment::new(format!("{glyph} "), Style::foreground(color).bold()),
        Segment::new(slug, tier_style(tier, color)),
    ];
    if !effort.is_empty() {
        segments.push(Segment::new(
            format!("({effort})"),
            Style::foreground(effort_color(effort)).bold(),
        ));
    }
    segments.push(Segment::new(
        format!(" {}", compact_count(context.metrics.used_tokens)),
        Style::foreground(context.palette.readout()).bold(),
    ));
    segments.push(Segment::new(
        format!("/{}", window_label(context.metrics.window_tokens)),
        Style::foreground(NEUTRAL_MUTED),
    ));
    segments
}

#[must_use]
pub fn workspace_identity(context: &RenderContext<'_>) -> Vec<Segment> {
    let directory = shorten_path_tail(&context.payload.workspace.current_dir);
    vec![Segment::new(
        directory,
        Style::foreground(NEUTRAL_READOUT).bold(),
    )]
}

#[must_use]
pub fn edit_volume(context: &RenderContext<'_>) -> Vec<Segment> {
    let added = compact_count(context.payload.cost.total_lines_added);
    let removed = compact_count(context.payload.cost.total_lines_removed);
    vec![
        Segment::new(format!("+{added}"), Style::foreground(ACCENT_GREEN)),
        Segment::new("/", Style::foreground(NEUTRAL_MUTED)),
        Segment::new(format!("-{removed}"), Style::foreground(ACCENT_RED)),
    ]
}

#[must_use]
pub fn context_tape(context: &RenderContext<'_>, width: usize) -> Vec<Segment> {
    let configuration = context.configuration;
    let glyphs = &configuration.glyphs;
    let percentage = context.eased.context_percentage;
    let needle_position = percentage / 100.0 * float_from_count(width);
    let needle_index = count_from_float(needle_position, width);
    let marker_index = count_from_float(
        configuration.compaction_threshold_percentage / 100.0 * float_from_count(width),
        width,
    );
    let past_threshold = percentage >= configuration.compaction_threshold_percentage;
    let fill_color = ACCENT_CYAN.blended_with(context.palette.bright(), context.palette.health());
    let shadow_color = Rgb::new(0x0e, 0x5a, 0x66).blended_with(context.palette.shadow(), 0.6);
    let tick_spacing = (width / 10).max(1);

    let mut cells = Vec::with_capacity(width);
    for index in 0..width {
        if index == marker_index {
            cells.push(compaction_marker(context, past_threshold));
            continue;
        }
        if index == needle_index {
            cells.push(Segment::new("▉", Style::foreground(fill_color).bold()));
        } else if index < needle_index {
            let shimmer = context.shimmer_at(index, width);
            let color = shadow_color.blended_with(fill_color, shimmer.mul_add(0.48, 0.42));
            cells.push(Segment::new(
                glyphs.tape_filled.clone(),
                Style::foreground(color),
            ));
        } else if configuration.auto_compact_enabled && index > marker_index {
            cells.push(Segment::new(
                glyphs.tape_track.clone(),
                Style::foreground(DANGER_TRACK),
            ));
        } else if index % tick_spacing == 0 {
            cells.push(Segment::new(
                glyphs.tape_tick.clone(),
                Style::foreground(NEUTRAL_TRACK),
            ));
        } else {
            cells.push(Segment::new(
                glyphs.tape_track.clone(),
                Style::foreground(NEUTRAL_TRACK),
            ));
        }
    }
    cells
}

fn compaction_marker(context: &RenderContext<'_>, past_threshold: bool) -> Segment {
    let glyph = context.configuration.glyphs.compaction_marker.clone();
    if !context.configuration.auto_compact_enabled {
        return Segment::new(glyph, Style::foreground(Rgb::new(0x3d, 0x46, 0x53)));
    }
    let color = if past_threshold && context.blinking() {
        ALARM_FLASH
    } else {
        ACCENT_ORANGE
    };
    Segment::new(glyph, Style::foreground(color).bold())
}

#[must_use]
pub fn context_readout(context: &RenderContext<'_>) -> Vec<Segment> {
    let percentage = percent_from_float(context.eased.context_percentage);
    vec![Segment::new(
        format!(" {percentage:>3}%"),
        Style::foreground(context.palette.readout()).bold(),
    )]
}

#[must_use]
pub fn activity_counters(context: &RenderContext<'_>) -> Vec<Segment> {
    let glyphs = &context.configuration.glyphs;
    let counters = context.counters;
    let mut segments = Vec::new();

    if context
        .payload
        .thinking
        .is_some_and(|thinking| thinking.enabled)
    {
        segments.push(Segment::new(
            format!("{} ", spinner_frame(context.frame)),
            Style::foreground(ACCENT_ORANGE).bold(),
        ));
    } else {
        segments.push(Segment::new(
            format!("{} ", glyphs.idle_marker),
            Style::foreground(NEUTRAL_MUTED),
        ));
    }

    segments.push(Segment::new(
        format!("{} ", glyphs.tool_calls),
        Style::foreground(NEUTRAL_LABEL),
    ));
    segments.push(Segment::new(
        counters.tool_calls.to_string(),
        Style::foreground(NEUTRAL_READOUT).bold(),
    ));
    segments.push(Segment::new(
        format!(" {} ", glyphs.turns),
        Style::foreground(NEUTRAL_LABEL),
    ));
    segments.push(Segment::new(
        counters.assistant_turns.to_string(),
        Style::foreground(NEUTRAL_READOUT).bold(),
    ));
    if counters.subagents > 0 {
        segments.push(Segment::new(
            format!(" {} ", glyphs.subagents),
            Style::foreground(NEUTRAL_LABEL),
        ));
        segments.push(Segment::new(
            counters.subagents.to_string(),
            Style::foreground(ACCENT_MAGENTA).bold(),
        ));
    }
    if counters.tool_errors > 0 {
        let color = if context.blinking() {
            ACCENT_RED
        } else {
            ALARM_FLASH
        };
        segments.push(Segment::new(
            format!(" {} ", glyphs.errors),
            Style::foreground(NEUTRAL_LABEL),
        ));
        segments.push(Segment::new(
            counters.tool_errors.to_string(),
            Style::foreground(color).bold(),
        ));
    }
    segments
}

#[must_use]
pub fn window_slot(context: &RenderContext<'_>) -> Vec<Segment> {
    let (label, percentage, reset_epoch) = choose_window(context);
    let glyphs = &context.configuration.glyphs;
    let critical = context
        .configuration
        .alert_thresholds
        .window_critical_percentage;
    let filled = percentage / 100.0 * float_from_count(GAUGE_CELLS);
    let whole_cells = count_from_float(filled, GAUGE_CELLS);
    let partial = filled - float_from_count(whole_cells);
    let on_color = ACCENT_CYAN.blended_with(context.palette.bright(), context.palette.health());

    let mut segments = vec![
        Segment::new(format!("{label} "), Style::foreground(NEUTRAL_LABEL).bold()),
        Segment::new(glyphs.gauge_open.clone(), Style::foreground(NEUTRAL_FRAME)),
    ];
    for index in 0..GAUGE_CELLS {
        if index < whole_cells {
            let alarming = percentage >= critical && (context.frame + index as u64) % 6 < 3;
            let color = if alarming {
                ACCENT_RED
            } else {
                Rgb::new(0x0e, 0x5a, 0x66)
                    .blended_with(on_color, float_from_count(index) / 20.0 + 0.55)
            };
            segments.push(Segment::new("█", Style::foreground(color)));
        } else if index == whole_cells && partial > 0.0 {
            segments.push(Segment::new(
                horizontal_partial(partial),
                Style::foreground(on_color),
            ));
        } else {
            segments.push(Segment::new(
                glyphs.gauge_empty.clone(),
                Style::foreground(NEUTRAL_TRACK),
            ));
        }
    }
    segments.push(Segment::new(
        glyphs.gauge_close.clone(),
        Style::foreground(NEUTRAL_FRAME),
    ));

    let rounded = percent_from_float(percentage);
    let readout_style = if percentage >= critical {
        Style::foreground(ACCENT_RED).bold()
    } else {
        Style::foreground(NEUTRAL_READOUT)
    };
    segments.push(Segment::new(format!("{rounded:>3}%"), readout_style));
    if let Some(epoch) = reset_epoch {
        let countdown = countdown_to_epoch(epoch, context.now_epoch_seconds);
        segments.push(Segment::new(
            format!(" {} {countdown}", glyphs.reset_clock),
            Style::foreground(NEUTRAL_MUTED),
        ));
    }
    segments
}

fn choose_window(context: &RenderContext<'_>) -> (&'static str, f32, Option<u64>) {
    let five = context.eased.five_hour_percentage;
    let seven = context.eased.seven_day_percentage;
    let notice = context
        .configuration
        .alert_thresholds
        .window_notice_percentage;
    let show_five_hour = match context.configuration.window_slot_strategy {
        WindowSlotStrategy::AlternateWindows => {
            (context.frame / context.configuration.alert_rotation_frames.max(1)) % 2 == 0
        }
        WindowSlotStrategy::ShowWorstWindow => five >= seven,
        WindowSlotStrategy::PreferFiveHourUntilSevenDayMatters => {
            !(seven >= notice && seven > five)
        }
    };
    if show_five_hour {
        ("5H", five, context.payload.five_hour_reset_epoch())
    } else {
        ("7D", seven, context.payload.seven_day_reset_epoch())
    }
}

#[must_use]
pub fn spend_readout(context: &RenderContext<'_>) -> Vec<Segment> {
    vec![Segment::new(
        format!("${:.2}", context.payload.cost.total_cost_usd),
        Style::foreground(ACCENT_SPEND),
    )]
}

#[must_use]
pub fn cache_readout(context: &RenderContext<'_>) -> Vec<Segment> {
    let percentage = percent_from_float(context.eased.cache_hit_ratio * 100.0);
    let poor =
        context.eased.cache_hit_ratio < context.configuration.alert_thresholds.cache_hit_warning;
    vec![
        Segment::new("HIT ", Style::foreground(NEUTRAL_LABEL)),
        Segment::new(
            format!("{percentage}%"),
            Style::foreground(if poor { ACCENT_AMBER } else { NEUTRAL_READOUT }),
        ),
    ]
}

#[must_use]
pub fn burn_readout(context: &RenderContext<'_>) -> Vec<Segment> {
    vec![Segment::new(
        format!("{:.1}k/m", context.eased.thousand_tokens_per_minute),
        Style::foreground(context.palette.readout()),
    )]
}

#[must_use]
pub fn alert_lane(context: &RenderContext<'_>, width: usize) -> Vec<Segment> {
    let glyphs = &context.configuration.glyphs;
    let Some(alert) = rotating_alert(
        context.alerts,
        context.frame,
        context.configuration.alert_rotation_frames,
    ) else {
        return vec![
            Segment::new(
                format!("{} ", glyphs.healthy),
                Style::foreground(ACCENT_GREEN),
            ),
            Segment::new("clean", Style::foreground(NEUTRAL_MUTED)),
        ];
    };

    let (glyph, color) = match alert.severity {
        AlertSeverity::Notice => (glyphs.alert_notice.as_str(), NEUTRAL_READOUT),
        AlertSeverity::Warning => (glyphs.alert_warning.as_str(), ACCENT_AMBER),
        AlertSeverity::Critical => (glyphs.alert_critical.as_str(), ACCENT_RED),
    };
    let glyph_color = if alert.severity == AlertSeverity::Critical && context.blinking() {
        ALARM_FLASH
    } else {
        color
    };

    let mut segments = vec![
        Segment::new(format!("{glyph} "), Style::foreground(glyph_color).bold()),
        Segment::new(format!("{} ", alert.label), Style::foreground(color).bold()),
        Segment::new(alert.detail.clone(), Style::foreground(NEUTRAL_MUTED)),
    ];
    if context.alerts.len() > 1 {
        segments.push(Segment::new(
            format!(" {}", rotation_pips(context)),
            Style::foreground(NEUTRAL_TRACK),
        ));
    }

    let mut used = 0;
    segments.retain_mut(|segment| {
        let remaining = width.saturating_sub(used);
        if remaining == 0 {
            return false;
        }
        if segment.width() > remaining {
            *segment = segment.truncated_to(remaining);
        }
        used += segment.width();
        true
    });
    segments
}

fn rotation_pips(context: &RenderContext<'_>) -> String {
    let period = context.configuration.alert_rotation_frames.max(1);
    let active =
        usize::try_from((context.frame / period) % context.alerts.len() as u64).unwrap_or(0);
    (0..context.alerts.len())
        .map(|index| if index == active { '•' } else { '·' })
        .collect()
}

const fn mode_color(mode: PermissionMode) -> Rgb {
    match mode {
        PermissionMode::Default => Rgb::new(0x39, 0x40, 0x4e),
        PermissionMode::Plan => ACCENT_CYAN,
        PermissionMode::AcceptEdits => ACCENT_GREEN,
        PermissionMode::Auto | PermissionMode::DontAsk => ACCENT_SKY,
        PermissionMode::BypassPermissions => ACCENT_RED,
    }
}

const fn tier_color(tier: ModelTier) -> Rgb {
    match tier {
        ModelTier::Haiku => ACCENT_SAGE,
        ModelTier::Sonnet => ACCENT_SKY,
        ModelTier::Fable => ACCENT_MINT,
        ModelTier::Opus | ModelTier::Unknown => ACCENT_MAGENTA,
    }
}

const fn tier_style(tier: ModelTier, color: Rgb) -> Style {
    let base = Style::foreground(color);
    match tier {
        ModelTier::Haiku => base.italic(),
        ModelTier::Sonnet => base.italic().underline(),
        ModelTier::Fable | ModelTier::Opus | ModelTier::Unknown => base.bold(),
    }
}

fn model_slug(payload: &StatusLinePayload) -> String {
    let source = if payload.model.display_name.is_empty() {
        &payload.model.id
    } else {
        &payload.model.display_name
    };
    let cleaned: String = source
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '.')
        .collect();
    if cleaned.is_empty() {
        "model".to_owned()
    } else {
        cleaned.to_ascii_lowercase()
    }
}
