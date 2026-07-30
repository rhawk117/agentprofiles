use crate::config::Configuration;
use crate::metrics::alerts::evaluate_alerts;
use crate::metrics::derived::DerivedMetrics;
use crate::metrics::health::score_session_health;
use crate::payload::StatusLinePayload;
use crate::render::cell::{Line, Segment, Style};
use crate::render::layout::{PriorityGroup, fit_to_width};
use crate::render::widgets::{
    RenderContext, activity_counters, alert_lane, burn_readout, cache_readout, context_readout,
    context_tape, edit_volume, mode_badge, mode_rail, model_identity, spend_readout, window_slot,
    workspace_identity,
};
use crate::state::SessionState;
use crate::theme::glyphs::{ModelTier, PermissionMode};
use crate::theme::palette::{ACCENT_GREEN, NEUTRAL_MUTED, TemperaturePalette};
use crate::transcript::ActivityCounters;

const TAPE_RESERVED_COLUMNS: usize = 34;
const TAPE_MINIMUM_CELLS: usize = 16;
const TAPE_MAXIMUM_CELLS: usize = 50;
const ALERT_LANE_RESERVED_COLUMNS: usize = 54;
const ALERT_LANE_MINIMUM_COLUMNS: usize = 14;

#[derive(Debug, Clone)]
pub struct RenderedStatusLine {
    pub lines: Vec<Line>,
}

impl RenderedStatusLine {
    #[must_use]
    pub fn to_ansi(&self) -> String {
        self.lines
            .iter()
            .map(Line::to_ansi)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[must_use]
    pub fn to_plain_text(&self) -> String {
        self.lines
            .iter()
            .map(Line::to_plain_text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug)]
pub struct RenderInputs<'a> {
    pub payload: &'a StatusLinePayload,
    pub configuration: &'a Configuration,
    pub state: &'a SessionState,
    pub counters: &'a ActivityCounters,
    pub mode: PermissionMode,
    pub columns: usize,
    pub now_epoch_seconds: u64,
}

#[must_use]
pub fn compose_status_line(inputs: &RenderInputs<'_>) -> RenderedStatusLine {
    let metrics = DerivedMetrics::from_payload(inputs.payload, inputs.state);
    let health = score_session_health(&inputs.state.eased, inputs.configuration);
    let alerts = evaluate_alerts(
        &metrics,
        &inputs.state.eased,
        inputs.state,
        inputs.configuration,
    );
    let context = RenderContext {
        configuration: inputs.configuration,
        payload: inputs.payload,
        metrics: &metrics,
        eased: &inputs.state.eased,
        counters: inputs.counters,
        alerts: &alerts,
        palette: TemperaturePalette::from_health(health),
        mode: inputs.mode,
        tier: ModelTier::from_model_identifier(
            &inputs.payload.model.id,
            &inputs.payload.model.display_name,
        ),
        frame: inputs.state.frame,
        now_epoch_seconds: inputs.now_epoch_seconds,
    };

    RenderedStatusLine {
        lines: vec![
            identity_row(&context, inputs.columns),
            context_row(&context, inputs.columns),
            resource_row(&context, inputs.columns),
        ],
    }
}

fn separator() -> Vec<Segment> {
    vec![Segment::new("  ▏ ", Style::foreground(NEUTRAL_MUTED))]
}

fn identity_row(context: &RenderContext<'_>, columns: usize) -> Line {
    let mut branch_group = separator();
    branch_group.push(Segment::new(
        format!("{} ", context.configuration.glyphs.branch),
        Style::foreground(ACCENT_GREEN),
    ));
    branch_group.push(Segment::new(
        branch_name(context),
        Style::foreground(ACCENT_GREEN),
    ));

    let mut workspace_group = separator();
    workspace_group.extend(workspace_identity(context));

    let mut edits_group = vec![Segment::plain("  ")];
    edits_group.extend(edit_volume(context));

    fit_to_width(
        vec![
            PriorityGroup::new(0, [mode_rail(context, 0), mode_badge(context)].concat()),
            PriorityGroup::new(
                0,
                [vec![Segment::plain(" ")], model_identity(context)].concat(),
            ),
            PriorityGroup::new(2, workspace_group),
            PriorityGroup::new(1, branch_group),
            PriorityGroup::new(1, edits_group),
        ],
        columns,
    )
}

fn context_row(context: &RenderContext<'_>, columns: usize) -> Line {
    let tape_cells = columns
        .saturating_sub(TAPE_RESERVED_COLUMNS)
        .clamp(TAPE_MINIMUM_CELLS, TAPE_MAXIMUM_CELLS);

    let mut tape_group = mode_rail(context, 1);
    tape_group.push(Segment::new(
        "CTX ",
        Style::foreground(context.palette.readout()).bold(),
    ));
    tape_group.push(Segment::new(
        context.configuration.glyphs.gauge_open.clone(),
        Style::foreground(NEUTRAL_MUTED),
    ));
    tape_group.extend(context_tape(context, tape_cells));
    tape_group.push(Segment::new(
        context.configuration.glyphs.gauge_close.clone(),
        Style::foreground(NEUTRAL_MUTED),
    ));
    tape_group.extend(context_readout(context));

    let mut counters_group = vec![Segment::plain("  ")];
    counters_group.extend(activity_counters(context));

    fit_to_width(
        vec![
            PriorityGroup::new(0, tape_group),
            PriorityGroup::new(1, counters_group),
        ],
        columns,
    )
}

fn resource_row(context: &RenderContext<'_>, columns: usize) -> Line {
    let lane_columns = columns
        .saturating_sub(ALERT_LANE_RESERVED_COLUMNS)
        .max(ALERT_LANE_MINIMUM_COLUMNS);

    let mut window_group = mode_rail(context, 2);
    window_group.extend(window_slot(context));

    let mut spend_group = vec![Segment::plain("  ")];
    spend_group.extend(spend_readout(context));

    let mut cache_group = vec![Segment::plain("  ")];
    cache_group.extend(cache_readout(context));

    let mut burn_group = vec![Segment::plain("  ")];
    burn_group.extend(burn_readout(context));

    let mut lane_group = vec![Segment::plain("  ")];
    lane_group.extend(alert_lane(context, lane_columns));

    fit_to_width(
        vec![
            PriorityGroup::new(0, window_group),
            PriorityGroup::new(2, spend_group),
            PriorityGroup::new(3, cache_group),
            PriorityGroup::new(4, burn_group),
            PriorityGroup::new(0, lane_group),
        ],
        columns,
    )
}

fn branch_name(context: &RenderContext<'_>) -> String {
    crate::git::current_branch(std::path::Path::new(&context.payload.workspace.current_dir))
        .unwrap_or_else(|| "no-branch".to_owned())
}
