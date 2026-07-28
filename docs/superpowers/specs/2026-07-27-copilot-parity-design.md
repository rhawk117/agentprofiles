# Bringing the Copilot tree to parity with the Claude one

*2026-07-27*

## The problem

This repo carries harness configuration for two agents. The `claude/` tree is mature and
installed. The `copilot/` tree was authored in a single commit and had never run — not
once, and not partially. Five independent things were wrong with it at the same time:

1. Sixteen files used Python 2 `except A, B:` syntax, a hard `SyntaxError` on Python 3.
2. `settings.json` was not valid JSON — a `// ...redacted ;)` comment sat inside an array.
3. Every configured path pointed at `$HOME/copilot`, missing the leading dot. The real
   config directory is `$HOME/.copilot`.
4. Nothing was executable.
5. A hook config referenced a script that did not exist in the tree.

Underneath those, the statusline's billing was built on a SQLite schema
(`assistant_usage_events`) that appears in no documentation and no community write-up. The
database it queried is real; the table is not.

The goal was a Copilot statusline that reports what prompts and sessions actually cost,
the two hooks the user named, and a configuration that mirrors the Claude one.

## Constraint that shaped everything

**Copilot CLI is not installed on this machine.** Nothing could be observed executing.
Every design decision had to be verifiable from fixtures and static checks alone, and no
step could rest on "run it and look".

That constraint is also the largest unmitigated risk in the result. The fixtures are
hand-authored from GitHub's documentation. If a payload key name is wrong, every assertion
still passes while the real hook silently does nothing. Each fixture carries a `_source`
key saying so. Capturing one live payload per event is the only thing that closes it.

## Billing: price from the payload

The statusline payload already carries everything needed, and one field that is
authoritative in a way our own arithmetic can never be.

| Source | What it gives | Standing |
|---|---|---|
| `ai_used.formatted` / `total_nano_aiu` | credits GitHub will bill | authoritative |
| `context_window.total_*_tokens` × `model-rates.json` | dollars, per token class | our estimate |
| `session-store.db` | cost data | rejected |

The database was rejected on grounds that have nothing to do with effort. GitHub publishes
no schema, marks the file automatically managed, and warns it changes between releases.
Three variants were considered and dropped: SQLite as primary (unverifiable), payload
primary with SQLite fallback (untestable code for no verified gain), and a runtime
`sqlite_master` prober (real complexity, no confirmed payoff).

Two corrections to what the original code assumed:

- `NANO_AIU_PER_AIC = 1_000_000_000` and `CREDITS_PER_USD = 100.0` were both **correct**.
  One AI credit is one US cent. The bug was never the conversion — it was reading the
  numbers out of an invented table instead of the payload.
- `total_input_tokens` is a **cumulative** sum that exceeds the window size. Using it for a
  per-message figure would inflate that figure without bound as a session grows.
  `last_call_input_tokens` is the per-turn number.

Derived figures: session dollars from all four token classes at their own rates,
`~$/msg` from `last_call_input_tokens` (a lower bound — it charges nothing for the output
the turn will produce), `$/hr` over `cost.total_duration_ms`, and the cache hit ratio,
which is the main efficiency lever a user can actually pull.

**No budget gauge.** The plan is enterprise with no usage cap, so a percentage-of-quota bar
would be measuring against nothing. Absolute figures instead.

An unpriced model prints its token volume and says there is no rate for it. Nothing
fabricates a price, and nothing claims a premium rate — the retired request-based plans
priced models with a multiplier that `model.display_name` still carries, and the same false
premium had already been removed from the Claude statusline.

## Hooks: Copilot's contract inverts Claude's

Two differences are load-bearing rather than cosmetic.

**`preToolUse` is fail-closed on exit status.** A non-zero exit denies the tool call even
when stdout said `allow`. A `grep` non-match propagating under `set -e` would be enough.
The guard therefore exits 0 on every path, including when it fails closed — a guard that
ever crashed would block every tool call for the rest of the session. Timeouts, by
contrast, fail open. This inverts Claude's semantics, where the guard fails soft.

**The verdict is flat.** `{"permissionDecision": ..., "permissionDecisionReason": ...}`,
with no `hookSpecificOutput` wrapper. The nested form is what caused `rtk-ai/rtk#3037`.

Verdicts split two ways, mirroring how Claude splits `permissions.deny` from
`permissions.ask`: `deny` for the literal credential-path list, `ask` for the fuzzier Bash
heuristics. The whole deny list lives inside the guard because Copilot's `permissions`
object supports only `disableBypassPermissionsMode` — which means the guard must see every
tool call, at a cost of roughly one process spawn per tool.

**Session context needs two phases, not one.** `userPromptSubmitted` looked like the
natural place for a first-prompt fallback, but its stdout is not processed. The mechanism
is `sessionStart` → `additionalContext` to capture and cache the banner, then
`userPromptTransformed` → `modifiedTransformedPrompt` to inject it. The pending file is
claimed with `os.replace`, which is atomic, so of any number of concurrent injects exactly
one wins the rename. A sentinel in the prompt prevents a second banner stacking on the first.

`preCompact` stdout is likewise not processed, so `compaction_snapshot.py` may only write a
file there and re-inject later.

## Rate tables stay different on purpose

`claude/model-rates.json` keys by API model ID (`claude-opus-4-8`).
`copilot/model-rates.json` keys by Copilot's display name (`claude-opus-4.8`). Only the
*schema* was unified — the key spellings must not be synchronized, and the `q_score`
columns come from different scales and are not comparable across the two files.

Every GPT and Gemini entry bills cache writes at `0.0`, so the cache-write TTL alert
self-suppresses on the default model without any special casing.

## Installer and tests

`install.sh` gates on the smoke suite and refuses to install anything when it fails. With
one combined suite, a red Copilot assertion would have blocked installing the working
Claude tree. The suite was therefore split per tree behind a `SMOKE_TREE` dispatcher
*before* any Copilot assertion was written, and `--tree` narrows the gate to what is being
installed.

`~/.copilot` is a live product directory holding `session-store.db`, `session-state/`, and
`logs/`. The installer names its managed directories exactly and never enumerates the
config-directory root. `copilot-instructions.md` stays unmanaged while it is empty:
symlinking it over a real one would blank the user's instructions.

`--print-manifest` exists so the suite can freeze the Claude manifest and prove the
two-tree refactor did not move it.

## What was not investigated

- The runtime behaviour of any Copilot hook. Nothing was observed executing.
- The `session-store.db` schema, which cannot be inspected without an install.
- Whether `total_reasoning_tokens` is additive to or already inside
  `total_output_tokens`. The code assumes included, which biases the figure low — the safe
  direction — and says so in a comment.
- The exact nesting of `total_nano_aiu`, top-level versus under `ai_used`. The code probes
  both.
- The `notification` event's payload key names. The hook tolerates a missing `title` or
  `message`.
