use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::ExitCode;

use instrumentline::config::Configuration;
use instrumentline::metrics::derived::DerivedMetrics;
use instrumentline::render::rows::{RenderInputs, compose_status_line};
use instrumentline::state::{SessionState, current_epoch_millis};

const USAGE: &str = "\
instrumentline - animated, health-reactive status line for Claude Code

usage:
  instrumentline render     read the status line payload on stdin and print three rows
  instrumentline doctor     report resolved configuration, paths and terminal width
  instrumentline version    print the version
  instrumentline help       print this message

environment:
  COLUMNS                     terminal width supplied by Claude Code
  INSTRUMENTLINE_CONFIG       path to a configuration file
  INSTRUMENTLINE_STATE_DIR    directory holding per-session animation state
";

fn main() -> ExitCode {
    let command = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "render".to_owned());
    match command.as_str() {
        "render" => run_render(),
        "doctor" => run_doctor(),
        "version" | "--version" | "-V" => {
            println!("instrumentline {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("instrumentline: unknown command '{other}'");
            print!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn run_render() -> ExitCode {
    let mut raw_input = String::new();
    if std::io::stdin().read_to_string(&mut raw_input).is_err() {
        raw_input.clear();
    }

    let rendered = render_payload(&raw_input);
    let mut stdout = std::io::stdout().lock();
    if writeln!(stdout, "{rendered}").is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn render_payload(raw_input: &str) -> String {
    let payload = instrumentline::StatusLinePayload::parse_lenient(raw_input);
    let configuration = Configuration::load_from_environment();
    let state_directory = Configuration::state_directory();

    let mut state = SessionState::load(&state_directory, &payload.session_id);
    let mode = SessionState::read_permission_mode(&state_directory, &payload.session_id);

    if !payload.transcript_path.is_empty() {
        state.counters = state
            .counters
            .advanced_from(Path::new(&payload.transcript_path));
    }

    let metrics = DerivedMetrics::from_payload(&payload, &state);
    state.record_cache_write_turn(metrics.cache_creation_tokens);
    state.advance_towards(metrics.as_sample(), configuration.easing_alpha_clamped());

    let counters = state.counters;
    let rendered = compose_status_line(&RenderInputs {
        payload: &payload,
        configuration: &configuration,
        state: &state,
        counters: &counters,
        mode,
        columns: configuration.terminal_columns(),
        now_epoch_seconds: epoch_seconds(),
    });

    state.last_sample_epoch_millis = current_epoch_millis();
    state.last_total_cost_usd = payload.cost.total_cost_usd;
    state.last_total_tokens = payload
        .context_window
        .total_input_tokens
        .saturating_add(payload.context_window.total_output_tokens);
    state.persist(&state_directory, &payload.session_id);

    rendered.to_ansi()
}

fn run_doctor() -> ExitCode {
    let configuration = Configuration::load_from_environment();
    let configuration_path = Configuration::discover_path();
    let state_directory = Configuration::state_directory();

    println!("instrumentline {}", env!("CARGO_PKG_VERSION"));
    println!(
        "configuration   {}",
        configuration_path.as_deref().map_or_else(
            || "built-in defaults".to_owned(),
            |path| path.display().to_string()
        )
    );
    println!("state directory {}", state_directory.display());
    println!("columns         {}", configuration.terminal_columns());
    println!(
        "easing alpha    {:.2}",
        configuration.easing_alpha_clamped()
    );
    println!(
        "auto compact    {} at {:.0}%",
        if configuration.auto_compact_enabled {
            "on"
        } else {
            "off"
        },
        configuration.compaction_threshold_percentage
    );
    println!("window slot     {:?}", configuration.window_slot_strategy);
    println!(
        "model glyphs    {}",
        configuration.glyphs.model_tiers.join(" ")
    );
    println!(
        "mode glyphs     {}",
        configuration.glyphs.permission_modes.join(" ")
    );
    println!(
        "state writable  {}",
        if state_directory.exists() {
            "yes"
        } else {
            "created on first render"
        }
    );
    ExitCode::SUCCESS
}

fn epoch_seconds() -> u64 {
    u64::try_from(current_epoch_millis() / 1_000).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::render_payload;

    #[test]
    fn rendering_empty_input_still_produces_three_rows() {
        assert_eq!(render_payload("").lines().count(), 3);
    }

    #[test]
    fn rendering_malformed_input_never_panics() {
        assert_eq!(render_payload("{ this is not json").lines().count(), 3);
    }
}
