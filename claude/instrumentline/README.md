# instrumentline

An animated, health-reactive status line for Claude Code. Three rows, truecolor ANSI, no Nerd Font
required.

Session health is a single weighted score over context pressure, the 5-hour rate-limit window, cache
hit ratio and burn rate. It is never printed as a label — it drives the colour temperature of the
tape, the token tank, the window gauge and the readouts simultaneously, so the whole line warms as
the session degrades.

## Rows

```
│▄  ⌖ PLAN  ⌬ opus4.8(high) 470k/1m  ▏ reconflux  ▏ ⎇ feat/statusline  +312/-87
│█ CTX ▕━━━━━━━━━━━━━▉╌╌┊╌╌╌╌┊╌╌╏╌╌╌▏  47%  ⠹ ⚒41 ↻19 ⧉3
│█ 5H ▕███▎······▏ 33% ↻2h09m  $1.84  HIT 89%  3.9k/m  ✓ clean
```

1. **Identity** — mode rail, fixed-width mode badge, model tier glyph, slug with tier typography,
   coloured effort, burned-to-window ratio, project, branch, lines added and removed.
2. **Context** — the tape, with a `╏` marker at the auto-compact threshold. With auto-compact on the
   track past the marker is drawn as dim red; with it off the marker dims and the danger zone
   disappears. Activity counters sit beside it.
3. **Resources** — one window slot shared by 5H and 7D, spend, cache hit rate, burn rate, alert lane.

## The rail and tank

Two columns down the left of all three rows. Column one is the permission mode: `│` default,
`╎` plan (breathing), `┃` accept-edits and auto, `║` bypass (blinking). Column two is a vertical
token tank filling bottom-up at 24 discrete levels, coloured by session health. On `/compact` it
drains over several frames rather than snapping.

## Why it looks smooth at 1 Hz

`refreshInterval` has a documented minimum of one second, so smooth animation is impossible by
construction. Three things make it read as fluid anyway:

- **Eased transitions.** Each invocation is stateless, so the binary persists the last *displayed*
  value per session and eases toward the true value at `easing_alpha` (default 0.60, about three
  frames to settle).
- **Sub-cell resolution.** Eighth-width blocks horizontally, eighth-height vertically.
- **A travelling shimmer** through the filled portion of gauges, five cells per frame.

## Install

From the repository root. This builds the crate, links the binary into `~/.claude/bin`, and installs
the hook alongside the other hooks:

```sh
./install.sh --tree claude
```

`settings.json` points at `~/.claude/bin/statusline`, a shim that runs this binary when it is built
and falls back to the Python status line when it is not, so a machine without a Rust toolchain still
gets a status line.

The installer writes none of the configuration below. It is what the repository's
`claude/settings.json` already contains, reproduced here for anyone wiring the crate up by hand:

```json
{
  "statusLine": {
    "type": "command",
    "command": "~/.claude/bin/statusline",
    "refreshInterval": 1,
    "padding": 0
  },
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "~/.claude/bin/hooks/write_permission_mode.sh" }] }
    ],
    "PreToolUse": [
      { "hooks": [{ "type": "command", "command": "~/.claude/bin/hooks/write_permission_mode.sh" }] }
    ]
  }
}
```

The hook exists because `permission_mode` is **not** in the status line payload — a mode change
triggers a re-render but the payload never says which mode you are in. Hook input does carry it, so
the hook writes it to `<state-dir>/<session-id>.mode` and the status line reads that file.

## Configuration

Optional, at `~/.claude/instrumentline.json` or wherever `INSTRUMENTLINE_CONFIG` points. Every key is
optional; unknown keys are rejected so typos surface immediately. Run `instrumentline doctor` to see
what actually resolved.

```json
{
  "easing_alpha": 0.6,
  "auto_compact_enabled": true,
  "compaction_threshold_percentage": 92.0,
  "window_slot_strategy": "prefer_five_hour_until_seven_day_matters",
  "glyphs": { "model_tiers": ["⌁", "❖", "☾", "⌬"] }
}
```

`window_slot_strategy` is one of `alternate_windows`, `show_worst_window`, or
`prefer_five_hour_until_seven_day_matters`.

## Data sources

| Shown | Source | Status |
| --- | --- | --- |
| context %, token counts, window size | `context_window` | doc-verified |
| cache hit ratio, cache writes | `context_window.current_usage` | doc-verified |
| 5h and 7d windows, reset times | `rate_limits` | doc-verified |
| spend, duration, lines added and removed | `cost` | doc-verified |
| model, effort | `model`, `effort.level` | doc-verified |
| permission mode | hook state file | doc-verified via hook input |
| branch | `.git/HEAD` read in-process | inferred, not in the payload |
| tool calls, turns, subagents, errors | transcript JSONL tail | inferred, undocumented format |
| auto-compact enabled and its threshold | configuration | inferred, not in the payload |

The transcript reader stores a byte offset in session state and only reads what was appended since
the last invocation, so cost does not grow with transcript length. It resets cleanly if the file is
truncated or rotated.

## Development

```sh
scripts/fmt.sh          # rustfmt and shfmt
scripts/fmt.sh --check  # verify without rewriting
scripts/lint.sh         # clippy (all/pedantic/nursery/cargo, -D warnings), rustdoc, shellcheck
scripts/test.sh         # cargo test, all targets
scripts/bench.sh        # per-invocation startup budget
scripts/check.sh        # the full gate
scripts/demo.sh         # render every fixture to your terminal
```

`unsafe` is forbidden. `unwrap`, `expect`, `panic`, `todo` and `unimplemented` are denied outside
tests: a status line that panics leaves a blank bar, so every failure path degrades to defaults
instead. Malformed or absent JSON still renders three rows.

## Terminal notes

Glyphs are box-drawing, blocks and braille — no Nerd Font. Italic (SGR 3) and underline (SGR 4) carry
the model tier and are the only terminal-capability dependency beyond truecolor; Windows Terminal
supports both. Width is counted in characters, so any glyph that font-falls-back to a double-width
rendering will shift a row. Test a glyph set in your own terminal before committing to it, and
`GlyphTable::ascii_fallback` exists for terminals that cannot cope.
