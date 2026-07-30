#![expect(
    clippy::panic,
    reason = "an integration test should fail loudly when a fixture is unusable"
)]

use std::path::Path;

use instrumentline::config::{Configuration, WindowSlotStrategy};
use instrumentline::metrics::derived::DerivedMetrics;
use instrumentline::payload::StatusLinePayload;
use instrumentline::render::rows::{RenderInputs, RenderedStatusLine, compose_status_line};
use instrumentline::state::SessionState;
use instrumentline::theme::glyphs::PermissionMode;
use instrumentline::transcript::ActivityCounters;

const FIXTURES: [&str; 6] = [
    "fresh", "cruising", "churn", "longhaul", "squeeze", "critical",
];
const FIXED_EPOCH_SECONDS: u64 = 1_785_379_197;

fn load_fixture(name: &str) -> StatusLinePayload {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("fixture {} unreadable: {error}", path.display()));
    StatusLinePayload::parse_lenient(&raw)
}

fn settled_state(payload: &StatusLinePayload, frames: u64) -> SessionState {
    let mut state = SessionState::default();
    for _ in 0..frames {
        let metrics = DerivedMetrics::from_payload(payload, &state);
        state.record_cache_write_turn(metrics.cache_creation_tokens);
        state.advance_towards(metrics.as_sample(), 0.6);
    }
    state
}

fn render(
    payload: &StatusLinePayload,
    configuration: &Configuration,
    state: &SessionState,
    mode: PermissionMode,
    columns: usize,
) -> RenderedStatusLine {
    let counters = ActivityCounters {
        tool_calls: 47,
        assistant_turns: 19,
        tool_errors: 0,
        subagents: 2,
        byte_offset: 0,
    };
    compose_status_line(&RenderInputs {
        payload,
        configuration,
        state,
        counters: &counters,
        mode,
        columns,
        now_epoch_seconds: FIXED_EPOCH_SECONDS,
    })
}

#[test]
fn every_fixture_renders_exactly_three_rows() {
    let configuration = Configuration::default();
    for name in FIXTURES {
        let payload = load_fixture(name);
        let state = settled_state(&payload, 8);
        let rendered = render(
            &payload,
            &configuration,
            &state,
            PermissionMode::Default,
            100,
        );
        assert_eq!(rendered.lines.len(), 3, "{name} did not produce three rows");
    }
}

#[test]
fn no_row_ever_exceeds_the_column_budget() {
    let configuration = Configuration::default();
    for name in FIXTURES {
        let payload = load_fixture(name);
        let state = settled_state(&payload, 8);
        for columns in [40, 60, 80, 100, 120, 200] {
            let rendered = render(
                &payload,
                &configuration,
                &state,
                PermissionMode::Plan,
                columns,
            );
            for (index, line) in rendered.lines.iter().enumerate() {
                assert!(
                    line.width() <= columns,
                    "{name} row {index} overflowed {columns} columns with {}",
                    line.width()
                );
            }
        }
    }
}

#[test]
fn every_permission_mode_keeps_the_badge_the_same_width() {
    let configuration = Configuration::default();
    let payload = load_fixture("cruising");
    let state = settled_state(&payload, 8);
    let mut widths = Vec::new();
    for mode in [
        PermissionMode::Default,
        PermissionMode::Plan,
        PermissionMode::AcceptEdits,
        PermissionMode::Auto,
        PermissionMode::DontAsk,
        PermissionMode::BypassPermissions,
    ] {
        let rendered = render(&payload, &configuration, &state, mode, 120);
        let first = rendered.lines.first().expect("identity row missing");
        widths.push(
            first
                .to_plain_text()
                .chars()
                .take(11)
                .collect::<String>()
                .chars()
                .count(),
        );
    }
    assert!(
        widths.windows(2).all(|pair| pair[0] == pair[1]),
        "badge widths drift: {widths:?}"
    );
}

#[test]
fn the_identity_row_carries_the_burned_to_window_ratio() {
    let configuration = Configuration::default();
    let payload = load_fixture("cruising");
    let state = settled_state(&payload, 12);
    let rendered = render(
        &payload,
        &configuration,
        &state,
        PermissionMode::Default,
        120,
    );
    let identity = rendered
        .lines
        .first()
        .expect("identity row missing")
        .to_plain_text();
    assert!(
        identity.contains("470k/1m"),
        "ratio missing from: {identity}"
    );
    assert!(
        identity.contains("(high)"),
        "effort missing from: {identity}"
    );
}

#[test]
fn a_healthy_session_reports_a_clean_alert_lane() {
    let configuration = Configuration::default();
    let payload = load_fixture("fresh");
    let state = settled_state(&payload, 8);
    let rendered = render(
        &payload,
        &configuration,
        &state,
        PermissionMode::Default,
        120,
    );
    let resource = rendered
        .lines
        .get(2)
        .expect("resource row missing")
        .to_plain_text();
    assert!(
        resource.contains("clean"),
        "expected a clean lane, got: {resource}"
    );
}

#[test]
fn a_critical_session_surfaces_the_compaction_alert() {
    let configuration = Configuration::default();
    let payload = load_fixture("critical");
    let mut state = settled_state(&payload, 12);
    let mut seen = Vec::new();
    for frame in 0..40 {
        state.frame = frame;
        let rendered = render(
            &payload,
            &configuration,
            &state,
            PermissionMode::Default,
            120,
        );
        seen.push(
            rendered
                .lines
                .get(2)
                .expect("resource row missing")
                .to_plain_text(),
        );
    }
    assert!(
        seen.iter().any(|row| row.contains("COMPACT")),
        "compaction alert never rotated into view: {seen:?}"
    );
}

#[test]
fn disabling_auto_compact_removes_the_danger_track_from_the_tape() {
    let payload = load_fixture("cruising");
    let state = settled_state(&payload, 8);

    let enabled = Configuration::default();
    let disabled = Configuration {
        auto_compact_enabled: false,
        ..Configuration::default()
    };

    let with_marker = render(&payload, &enabled, &state, PermissionMode::Default, 120).to_ansi();
    let without_marker =
        render(&payload, &disabled, &state, PermissionMode::Default, 120).to_ansi();
    assert_ne!(
        with_marker, without_marker,
        "auto-compact toggle changed nothing"
    );
}

#[test]
fn the_window_slot_shows_seven_day_only_when_it_is_the_problem() {
    let configuration = Configuration {
        window_slot_strategy: WindowSlotStrategy::PreferFiveHourUntilSevenDayMatters,
        ..Configuration::default()
    };

    let cruising = load_fixture("cruising");
    let cruising_state = settled_state(&cruising, 12);
    let cruising_row = render(
        &cruising,
        &configuration,
        &cruising_state,
        PermissionMode::Default,
        120,
    )
    .lines
    .get(2)
    .expect("resource row missing")
    .to_plain_text();
    assert_eq!(
        window_slot_label(&cruising_row),
        "5H",
        "got: {cruising_row}"
    );

    let longhaul = load_fixture("longhaul");
    let longhaul_state = settled_state(&longhaul, 12);
    let longhaul_row = render(
        &longhaul,
        &configuration,
        &longhaul_state,
        PermissionMode::Default,
        120,
    )
    .lines
    .get(2)
    .expect("resource row missing")
    .to_plain_text();
    assert_eq!(
        window_slot_label(&longhaul_row),
        "7D",
        "got: {longhaul_row}"
    );
}

#[test]
fn easing_produces_intermediate_frames_rather_than_a_jump() {
    let payload = load_fixture("critical");
    let mut state = SessionState::default();
    let metrics = DerivedMetrics::from_payload(&payload, &state);
    state.advance_towards(metrics.as_sample(), 0.6);

    let drained = load_fixture("fresh");
    let mut observed = Vec::new();
    for _ in 0..4 {
        let next = DerivedMetrics::from_payload(&drained, &state);
        state.advance_towards(next.as_sample(), 0.6);
        observed.push(state.eased.context_percentage);
    }

    assert!(
        observed.windows(2).all(|pair| pair[1] < pair[0]),
        "not monotonic: {observed:?}"
    );
    assert!(
        observed[0] > 20.0,
        "first eased frame jumped straight to target: {observed:?}"
    );
}

#[test]
fn ansi_output_always_resets_after_a_styled_run() {
    let configuration = Configuration::default();
    for name in FIXTURES {
        let payload = load_fixture(name);
        let state = settled_state(&payload, 6);
        let rendered = render(
            &payload,
            &configuration,
            &state,
            PermissionMode::BypassPermissions,
            100,
        );
        for line in &rendered.lines {
            let text = line.to_ansi();
            let opens = text.matches("\u{1b}[").count();
            let resets = text.matches("\u{1b}[0m").count();
            assert_eq!(
                opens - resets,
                resets,
                "{name}: unbalanced escape sequences"
            );
        }
    }
}

fn window_slot_label(row: &str) -> String {
    row.chars()
        .skip_while(|character| !character.is_ascii_alphanumeric())
        .take(2)
        .collect()
}
