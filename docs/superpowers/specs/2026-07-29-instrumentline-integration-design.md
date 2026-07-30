# instrumentline integration

Replace the Python status line for the Claude tree with `claude/instrumentline`, a Rust status line
already written and sitting untracked in the repo. The installer has to learn how to handle a
compiled artifact, and the existing setup has to keep working on machines without a Rust toolchain.

## Established facts

Verified by running the code on 2026-07-29, not inferred from the README.

| Fact | Evidence |
| --- | --- |
| `cargo build --release` succeeds | 10.5s clean build, cargo 1.97.1 |
| Binary renders three rows and exits 0 | every fixture in `tests/fixtures/*.json` |
| Malformed and empty JSON still render three rows, exit 0 | `printf '{"model":{'` and `printf '{}'` |
| Default subcommand is `render` | `src/main.rs:28`, `unwrap_or_else(|| "render")` |
| `doctor` prints a parseable config dump ending in `state writable yes` | run directly |
| **`NO_COLOR` is not honoured** | no match for `NO_COLOR`, `TERM`, `is_terminal` anywhere in `src/` |
| Env vars read | `INSTRUMENTLINE_CONFIG`, `INSTRUMENTLINE_STATE_DIR`, `HOME`, columns var (`src/config.rs`) |
| Default state directory | `~/.claude/instrumentline-state/` (`src/config.rs:160`, `state_directory()`) |
| `target/` is already gitignored | `.gitignore:76`, 95M on disk |
| No test asserts on current glyph spacing | `tests/rendering.rs` references counts only, never rendered text |

The Claude tree currently links these paths (`install.sh` `MANAGED_FILES` / `MANAGED_DIRS`):
`settings.json`, `CLAUDE.md`, `model-rates.json`, `bin/statusline.py`, and the contents of
`bin/hooks`, `skills`, `agents`. `~/.claude/settings.json` is one of those symlinks, which is why
editing it during development changes the running session.

## Decisions

Each of these was settled by the user. The rejected options are recorded so they do not get reopened.

### Build at install time, link the artifact

`install.sh` runs `cargo build --release` and links
`~/.claude/bin/instrumentline` to `claude/instrumentline/target/release/instrumentline`.

Rejected: `cargo install --path` into `~/.local/bin`, which is what the crate README suggests. It
copies rather than links, so every rebuild would need a reinstall and `--check` could not audit the
result. Also rejected: committing a prebuilt binary, which puts a platform-specific artifact in a
dotfiles repo.

### The Python status line stays as a fallback

`claude/bin/statusline.py` remains in the repo and in the manifest. It is no longer the default. On a
machine with no `cargo`, or where the build fails, it is what renders.

Rejected: deleting it. A machine without Rust would get no status line, and the Claude-side smoke
coverage for model pricing and rate-limit replay would go with it.

### A shim selects between them at render time

**This is the load-bearing decision.** The obvious fallback design does not work: `settings.json` is
a version-controlled symlink, so an installer that rewrote `statusLine` per machine would corrupt the
repo.

Instead, `claude/bin/statusline` is a new executable bash file in `MANAGED_FILES`. `settings.json`
points at it permanently. It execs `~/.claude/bin/instrumentline render` when that is executable and
falls back to `python3 ~/.claude/bin/statusline.py` otherwise.

This also keeps the existing "every referenced script exists in the repo" smoke check passing. A
direct `~/.claude/bin/instrumentline` reference in `settings.json` would fail that check, because the
binary has no counterpart in the repo tree.

Note that a stray one-byte `~/.claude/statusline` file already exists. It is unrelated and unmanaged.
The shim installs at `~/.claude/bin/statusline`, a different path.

### The mode hook moves into the hooks directory

`claude/instrumentline/hooks/write-permission-mode.sh` moves to
`claude/bin/hooks/write_permission_mode.sh`. The underscore matches every other hook in that
directory, and hyphenated names under `~/.claude/hooks/` are exactly what the installer's
`LEGACY_HOOKS` retires.

The hook exists because `permission_mode` is absent from the status line payload but present in hook
input. It writes the mode to `<state-dir>/<session-id>.mode` for the status line to read.

### `instrumentline.json` stays unmanaged

Every key is optional and the defaults are sane. Leaving it out of the manifest means per-machine
tuning survives a reinstall. Document it in the README instead.

### The install gate renders fixtures, it does not run cargo test

`tests/smoke-claude.sh` pipes fixtures through the built binary and asserts on the output. It does
not run `scripts/check.sh`, which would block an install on a clippy nit unrelated to whether the
line renders.

### Animation settings follow the crate

`refreshInterval` 5 to 1, `padding` 1 to 0. The eased transitions, shimmer and breathing rail are
built around a 1Hz repaint and read as stutter at 5s.

The existing assertion that `refreshInterval` stays odd still passes at 1. That assertion guards the
Python line's pulse phase, which now only matters in the fallback path. Keep it, and say why in a
comment.

### Live-session safety

`~/.claude/settings.json` was replaced with a plain copy of its current contents at the start of this
work, so the running session keeps the working Python line while the repo changes. **Restoring that
symlink is the final step of implementation, after the gate passes.** The other links are harmless to
edit live.

## Changes

### 1. `claude/bin/statusline` (new)

Executable bash. Execs the binary when present, the Python line otherwise. Roughly:

```bash
#!/usr/bin/env bash
binary="${HOME}/.claude/bin/instrumentline"
[ -x "$binary" ] && exec "$binary" render
exec python3 "${HOME}/.claude/bin/statusline.py"
```

### 2. `install.sh`

`status_of()` and `manifest()` both assume `dest_rel == src_rel`. The binary breaks that assumption:
it lives at `instrumentline/target/release/instrumentline` in the source tree but installs to
`bin/instrumentline`. Add one per-tree array of `dest=src` pairs:

```bash
MANAGED_BUILT=(bin/instrumentline=instrumentline/target/release/instrumentline)
```

- `status_of()` takes an optional explicit source path as a second argument.
- `manifest()` emits the dest side, so `--check` and `--print-manifest` cover the binary.
- Add `bin/statusline` to `MANAGED_FILES`.
- New `--skip-build` flag.
- Build runs before the smoke suite, because the suite needs the binary.
- A missing `cargo` is a **warning, not an error**. Install proceeds and the shim falls back.

Keep bash 3.2 compatibility. The file already avoids `declare -n` for this reason, so use parallel
plain arrays rather than associative ones.

### 3. `claude/settings.json`

- `statusLine.command` to `~/.claude/bin/statusline`
- `statusLine.refreshInterval` to 1, `statusLine.padding` to 0
- `write_permission_mode.sh` on `UserPromptSubmit`
- `write_permission_mode.sh` on `PreToolUse` as a second, matcher-less entry alongside the existing
  Bash-matched `protected_paths_guard.sh`

### 4. Glyph spacing

Five sites in `src/render/widgets.rs` put a glyph directly against its number, so `⚒41` reads as one
token. Add a space at each, giving a single rule: glyph, space, value.

| Line | Now | After |
| --- | --- | --- |
| 271-274 | `⚒41` | `⚒ 41` |
| 279-282 | `↻19` | `↻ 19` |
| 288-291 | `⧉3` | `⧉ 3` |
| 303-306 | errors counter | spaced |
| 369 | `↻1h47m` | `↻ 1h47m` |

The model tier, branch, alert and health glyphs are already spaced and do not change.

This widens rows 2 and 3 by about five columns. Render every fixture at `COLUMNS=80` and `COLUMNS=100`
afterwards. If a fixture overflows at 80, report it rather than trimming something else to compensate.

### 5. `tests/smoke-claude.sh`

New `group "instrumentline"`, skipped cleanly when no binary exists:

- Each fixture renders three rows and exits 0.
- `{}` and malformed JSON render three rows and exit 0.
- `doctor` reports `state writable yes`.
- The shim picks the binary when present and Python when it is not.

Assertions must strip ANSI themselves, because the binary has no `NO_COLOR` support. The existing
Python assertions rely on `NO_COLOR=1` and cannot be copied directly.

**The frozen manifest assertion needs updating** for `bin/instrumentline`, `bin/statusline` and
`bin/hooks/write_permission_mode.sh`. It is a sorted space-joined string; regenerate it from
`install.sh --tree claude --print-manifest` rather than editing by hand.

### 6. `claude/instrumentline/README.md`

The README is wrong in two ways, in two separate subsections. Both must be fixed.

**The path is `~/.claude`, not `~/claude`.** The missing dot appears three times, and only the first
is inside the install section:

| Line | Context |
| --- | --- |
| 49 | `install -Dm755 hooks/write-permission-mode.sh ~/claude/bin/hooks/...` |
| 64 | the `UserPromptSubmit` hook command in the settings.json block |
| 67 | the `PreToolUse` hook command in the settings.json block |

Anyone who followed this README verbatim created a stray `~/claude/` directory and a hook that never
fired, so the status line would have shown the default permission mode forever. Nothing in the repo
currently references the wrong path, so this is documentation-only.

**The install and settings instructions are superseded.** Replace the install section with a pointer
to `./install.sh`, and update the settings.json block to the real wiring: `statusLine.command` of
`~/.claude/bin/statusline`, `refreshInterval` 1, `padding` 0, and the hook at
`~/.claude/bin/hooks/write_permission_mode.sh` (underscores, per the rename in change 2).

Grep for `~/claude` and `$HOME/claude` afterwards to confirm none survive.

## Verification

Confirmed to run in this repo:

```sh
cd claude/instrumentline && scripts/check.sh   # fmt, clippy, cargo test, shellcheck, rustdoc
bash tests/smoke.sh                            # both trees
SMOKE_TREE=claude bash tests/smoke.sh          # claude only
./install.sh --check                           # audit, no writes
./install.sh --tree claude --print-manifest    # regenerate the frozen assertion

# no missing-dot paths survive anywhere; must print nothing
grep -rn '~/claude\|\$HOME/claude' --exclude-dir=target .
```

Then, as the last step, restore the `~/.claude/settings.json` symlink and confirm the status line
renders in a live session.

## Known issues, deliberately not fixed

These are real and worth a follow-up, but none of them belong in this change.

- `~/.claude/instrumentline-state/` accumulates one `.json` and one `.mode` per session, with no
  pruning anywhere in `src/`. The `cleanupPeriodDays: 30` setting does not reach it.
- `GlyphTable::ascii_fallback()` at `src/theme/glyphs.rs:170` is unreachable. Nothing in config or
  env selects it, so the terminals it exists for cannot actually get it.
- The binary ignores `NO_COLOR`, which is a de facto standard and cheap to support.
- `copilot/statusline.py` and `claude/bin/statusline.py` are 667 and 670 lines of near-identical
  logic sharing `model-rates.json`. Out of scope here.

## Not investigated

- Whether the transcript reader's byte-offset tracking behaves correctly across `/compact`. The
  README claims it resets on truncation; this was not exercised.
- Rendering on a terminal without truecolor or without italic support.
- Any behaviour of the `copilot` tree, which this change does not touch.
