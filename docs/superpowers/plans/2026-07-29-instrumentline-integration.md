# instrumentline Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `claude/instrumentline` (a Rust status line) the default for the Claude tree, built and linked by `install.sh`, with the existing Python status line as an automatic fallback.

**Architecture:** `settings.json` points permanently at a small bash shim, `~/.claude/bin/statusline`, which execs the Rust binary when it is present and `statusline.py` when it is not. The installer gains a class of managed path whose source lives at a different relative path than its destination, because the binary is built into `target/release/` but installs to `bin/`. Selection happens per machine at render time, so no per-machine rewrite of the version-controlled `settings.json` is ever needed.

**Tech Stack:** bash 3.2-compatible shell, Rust (cargo 1.97.1), Python 3, Claude Code settings JSON.

**Design spec:** `docs/superpowers/specs/2026-07-29-instrumentline-integration-design.md`. Read it before starting. Every decision below was settled there and should not be reopened.

## Global Constraints

- Branch `feat/instrumentline` already exists and is checked out. Do not work on `main`.
- **`~/.claude/settings.json` is currently a plain file, not a symlink.** It was deliberately unlinked so this running Claude Code session keeps its working status line while the repo changes. Restoring the symlink is Task 8 and must not happen earlier.
- `install.sh` targets **bash 3.2** (macOS ships it). No associative arrays, no `declare -n` namerefs, no `${var^^}`.
- `install.sh` runs under `set -euo pipefail`. Expanding a possibly-empty array must use the `${arr[@]+"${arr[@]}"}` guard the file already applies to `LEGACY_HOOKS` at lines 155 and 220.
- The `instrumentline` binary **does not support `NO_COLOR`**. Any smoke assertion on its output must strip ANSI itself. Do not copy the `NO_COLOR=1` pattern used by the Python assertions.
- Do not run `cargo install`. Do not commit anything under `claude/instrumentline/target/` (already gitignored).
- Never edit the frozen manifest string by hand. Always regenerate it with `./install.sh --tree claude --print-manifest`.
- The whole crate `claude/instrumentline/` is currently **untracked**. Task 4 commits it.
- **Every command below assumes you start at the repository root, `/home/rhawk/dev/agentprofiles`.** Steps that need another directory use a subshell so the working directory never leaks. Inside `tests/smoke-claude.sh`, `$REPO` and `$CLAUDE` are already defined by the suite; in your own shell they are not, so run `REPO=/home/rhawk/dev/agentprofiles` first if you are pasting a step that uses them.

## File Structure

| Path | Status | Responsibility |
| --- | --- | --- |
| `claude/bin/statusline` | create | Bash shim: pick the Rust binary or fall back to Python |
| `claude/bin/hooks/write_permission_mode.sh` | create (moved) | Write permission mode to the state dir for the binary to read |
| `claude/instrumentline/hooks/write-permission-mode.sh` | delete | Superseded by the above |
| `install.sh` | modify | Build the crate; resolve dest→src for built artifacts |
| `claude/settings.json` | modify | Point at the shim; wire the mode hook |
| `claude/instrumentline/src/render/widgets.rs` | modify | Glyph spacing, 5 sites |
| `claude/instrumentline/README.md` | modify | Fix `~/claude` → `~/.claude`; replace install instructions |
| `tests/smoke-claude.sh` | modify | Frozen manifest string; new `instrumentline` group |
| `claude/instrumentline/**` | commit | Whole crate enters version control |

---

### Task 1: Unblock the installer by fixing the pre-existing manifest drift

`./install.sh` currently exits 1 before writing anything, because `tests/smoke-claude.sh:297` asserts a frozen manifest string that three earlier commits made stale. Nothing else in this plan can be verified until this is green.

Absorbing these three means **the installer will begin linking `agents/ci-watcher.md`, `bin/hooks/capper_simulate.py` and `bin/hooks/edit_capper.py` into `~/.claude`**. That is intended: `manifest()` auto-enumerates `MANAGED_DIRS`, and only the test was stale. Say so in the commit message so it is not a silent expansion of the installer's reach.

**Files:**
- Modify: `tests/smoke-claude.sh:295-298`

**Interfaces:**
- Consumes: nothing.
- Produces: a green `bash tests/smoke-claude.sh`, which every later task's verification depends on.

- [ ] **Step 1: Confirm the failure exists before touching anything**

Run: `bash tests/smoke-claude.sh 2>&1 | grep -A2 "installer manifest"`

Expected: `FAIL  the Claude manifest is unchanged`, listing a `got` string containing `agents/ci-watcher.md`, `bin/hooks/capper_simulate.py`, `bin/hooks/edit_capper.py`.

- [ ] **Step 2: Generate the correct string**

Run: `./install.sh --tree claude --print-manifest | cut -f2 | sort | tr '\n' ' '`

Expected output, exactly (note the trailing space):

```
CLAUDE.md agents/ci-watcher.md agents/code-analyst.md agents/engineer.md agents/plan-critic.md agents/scout.md bin/hooks/capper_simulate.py bin/hooks/edit_capper.py bin/hooks/notify.sh bin/hooks/protected_paths_guard.sh bin/hooks/read_logger.py bin/hooks/ruff_on_edit.py bin/hooks/session_context.sh bin/statusline.py model-rates.json settings.json skills/pysymbols 
```

- [ ] **Step 3: Replace the frozen string**

In `tests/smoke-claude.sh`, the assertion currently reads:

```bash
assert_eq "the Claude manifest is unchanged" \
  "$(bash "$REPO/install.sh" --tree claude --print-manifest | cut -f2 | sort | tr '\n' ' ')" \
  "CLAUDE.md agents/code-analyst.md agents/engineer.md agents/plan-critic.md agents/scout.md bin/hooks/notify.sh bin/hooks/protected_paths_guard.sh bin/hooks/read_logger.py bin/hooks/ruff_on_edit.py bin/hooks/session_context.sh bin/statusline.py model-rates.json settings.json skills/pysymbols "
```

Replace only the third argument with the string from Step 2. Leave the comment above it in place.

- [ ] **Step 4: Verify the suite is green**

Run: `bash tests/smoke-claude.sh`

Expected: `all claude smoke tests passed`, exit 0.

- [ ] **Step 5: Verify the installer can now audit**

Run: `./install.sh --check; echo "exit=$?"`

Expected: a per-path listing. Exit 0 or 1 are both acceptable here — 1 just means some paths are not yet linked. The point is that it no longer dies in the smoke gate.

- [ ] **Step 6: Commit**

```bash
git add tests/smoke-claude.sh
git commit -m "fix(tests): refresh the frozen Claude manifest

Three files added in earlier commits were never added to the frozen
assertion, so the smoke suite was red and install.sh exited before
writing anything.

This absorbs agents/ci-watcher.md, bin/hooks/capper_simulate.py and
bin/hooks/edit_capper.py into the manifest, which means the installer
now links them into ~/.claude. That is what the auto-enumerating
manifest always implied; only the test was stale."
```

---

### Task 2: Move the permission-mode hook into the hooks directory

The binary cannot see `permission_mode` — it is absent from the status line payload but present in hook input. The hook writes it to `<state-dir>/<session-id>.mode`.

The rename to underscores matches every other hook in `claude/bin/hooks/`. Hyphenated names under `~/.claude/hooks/` are exactly what the installer's `LEGACY_HOOKS` retires, so keeping the hyphen would invite confusion.

**Files:**
- Create: `claude/bin/hooks/write_permission_mode.sh`
- Delete: `claude/instrumentline/hooks/write-permission-mode.sh`
- Modify: `claude/settings.json` (hooks only, not statusLine)
- Modify: `tests/smoke-claude.sh` (frozen manifest string, new assertions)

**Interfaces:**
- Consumes: Task 1's green suite.
- Produces: `~/.claude/bin/hooks/write_permission_mode.sh`, wired to `UserPromptSubmit` and `PreToolUse`. Task 3 relies on the settings file already being valid JSON with these entries.

- [ ] **Step 1: Move the file, preserving the executable bit**

```bash
git mv claude/instrumentline/hooks/write-permission-mode.sh \
       claude/bin/hooks/write_permission_mode.sh 2>/dev/null \
  || mv claude/instrumentline/hooks/write-permission-mode.sh \
        claude/bin/hooks/write_permission_mode.sh
rmdir claude/instrumentline/hooks 2>/dev/null || true
chmod 755 claude/bin/hooks/write_permission_mode.sh
ls -l claude/bin/hooks/write_permission_mode.sh
```

Expected: mode `-rwxr-xr-x`. The `git mv` will fail because the crate is untracked; the `||` fallback handles that.

- [ ] **Step 2: Confirm the hook and the binary agree on the state directory**

The hook defaults to `${HOME}/.claude/instrumentline-state`; `Configuration::state_directory()` in `src/config.rs:160` must default to the same path, and both must honour `INSTRUMENTLINE_STATE_DIR`.

Run:

```bash
grep -n 'INSTRUMENTLINE_STATE_DIR\|instrumentline-state' \
  claude/bin/hooks/write_permission_mode.sh claude/instrumentline/src/config.rs
```

Expected: both files reference `INSTRUMENTLINE_STATE_DIR`, and both fall back to a path ending `instrumentline-state`. **If they disagree, stop and report** — the mode would silently never reach the status line.

- [ ] **Step 3: Write the failing assertions**

Append to `tests/smoke-claude.sh`, immediately before the `installer manifest` group:

```bash
# --------------------------------------------------------------------------
group "write_permission_mode.sh"

mode_hook() { printf '%s' "$1" | INSTRUMENTLINE_STATE_DIR="$TMP/modestate" "$HOOKS/write_permission_mode.sh"; }

rm -rf "$TMP/modestate"
mode_hook '{"session_id":"abc-123","permission_mode":"plan"}'
assert_eq "writes the mode for the session" "$(cat "$TMP/modestate/abc-123.mode" 2>/dev/null)" "plan"

mode_hook '{"session_id":"abc-123","permission_mode":"acceptEdits"}'
assert_eq "overwrites on mode change" "$(cat "$TMP/modestate/abc-123.mode" 2>/dev/null)" "acceptEdits"

# A path separator in the session id must not escape the state directory.
mode_hook '{"session_id":"../escape","permission_mode":"plan"}'
assert_eq "sanitises the session id" "$([ -e "$TMP/escape.mode" ] && echo leaked || echo contained)" "contained"

# Missing fields exit 0 without writing, so a hook failure never blocks a tool call.
out="$(mode_hook '{"session_id":"only-id"}')"
assert_eq "missing mode exits 0" "$?" "0"
assert_eq "missing mode writes nothing" "$([ -e "$TMP/modestate/only-id.mode" ] && echo wrote || echo silent)" "silent"
out="$(mode_hook 'not json')"
assert_eq "malformed input exits 0" "$?" "0"
```

- [ ] **Step 4: Run them and watch them pass**

Run: `bash tests/smoke-claude.sh 2>&1 | grep -A8 "write_permission_mode"`

Expected: all six assertions `ok`. These characterise behaviour the hook already has rather than driving new code, so they should pass immediately. **If "sanitises the session id" fails, stop and report** — that is a path-traversal write outside the state directory.

- [ ] **Step 5: Wire the hook into settings.json**

In `claude/settings.json`, inside `"hooks"`, add a `UserPromptSubmit` key and add a second, matcher-less entry to the existing `PreToolUse` array. The existing Bash-matched `protected_paths_guard.sh` entry must stay exactly as it is; a matcher-less sibling runs in addition to it and does not change its behaviour.

```json
"UserPromptSubmit": [
  {
    "hooks": [
      { "type": "command", "command": "~/.claude/bin/hooks/write_permission_mode.sh" }
    ]
  }
],
"PreToolUse": [
  {
    "matcher": "Bash",
    "hooks": [
      { "type": "command", "command": "~/.claude/bin/hooks/protected_paths_guard.sh" }
    ]
  },
  {
    "hooks": [
      { "type": "command", "command": "~/.claude/bin/hooks/write_permission_mode.sh" }
    ]
  }
],
```

- [ ] **Step 6: Regenerate the frozen manifest string**

Run: `./install.sh --tree claude --print-manifest | cut -f2 | sort | tr '\n' ' '`

Copy the output verbatim into the third argument of the `the Claude manifest is unchanged` assertion. It should now contain `bin/hooks/write_permission_mode.sh` and no longer be identical to Task 1's string.

- [ ] **Step 7: Verify the whole suite**

Run: `bash tests/smoke-claude.sh`

Expected: `all claude smoke tests passed`. The `settings.json wiring` group must report **8** referenced scripts (up from 7) and confirm all are executable.

- [ ] **Step 8: Commit**

```bash
git add claude/bin/hooks/write_permission_mode.sh claude/settings.json tests/smoke-claude.sh
git commit -m "feat(claude): wire the instrumentline permission-mode hook

permission_mode is absent from the status line payload but present in
hook input, so the hook records it per session for the status line to
read. Renamed to underscores to match the other hooks; hyphenated names
under ~/.claude/hooks are what LEGACY_HOOKS retires."
```

---

### Task 3: Add the shim and point settings.json at it

**This is the load-bearing piece.** The installer cannot rewrite `statusLine` per machine, because `settings.json` is itself a version-controlled symlink and doing so would corrupt the repo. The shim moves the choice to render time instead.

Ordering note: this task lands *before* the installer learns to build the binary. That is deliberate and safe — with no binary present, the shim falls back to Python, which is exactly today's behaviour. The status line keeps working throughout.

**Files:**
- Create: `claude/bin/statusline`
- Modify: `claude/settings.json` (statusLine block)
- Modify: `tests/smoke-claude.sh`

**Interfaces:**
- Consumes: Task 2's settings file.
- Produces: `~/.claude/bin/statusline`, the permanent `statusLine.command`. Task 4 relies on this exec'ing `$CLAUDE_HOME/bin/instrumentline render` once that path exists.

- [ ] **Step 1: Write the failing assertions**

Append to `tests/smoke-claude.sh`, before the `installer manifest` group:

```bash
# --------------------------------------------------------------------------
group "statusline shim"

# A private CLAUDE_HOME so the assertions never depend on what is really installed.
shim_home="$TMP/shimhome"
mkdir -p "$shim_home/bin"
cp "$CLAUDE/bin/statusline.py" "$shim_home/bin/statusline.py"

shim() { CLAUDE_HOME="$shim_home" "$CLAUDE/bin/statusline" <"$FIXTURES/statusline-full.json"; }

# The python line prints a verdict glyph; instrumentline does not. That is the discriminator.
out="$(shim)"
assert_eq "falls back to python when no binary is installed" "$?" "0"
assert_has "python line actually rendered" "$out" "★"

binary="$REPO/claude/instrumentline/target/release/instrumentline"
if [ -x "$binary" ]; then
  ln -sf "$binary" "$shim_home/bin/instrumentline"
  out="$(shim)"
  assert_eq "prefers instrumentline when it is installed" "$?" "0"
  assert_lacks "python line did not run" "$out" "★"
  assert_eq "instrumentline rendered three rows" "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" "3"
  rm -f "$shim_home/bin/instrumentline"
else
  printf '  \033[33mskip\033[0m  instrumentline not built (run cargo build --release)\n'
fi

# A non-executable file must not be selected; it would produce a blank status bar.
: >"$shim_home/bin/instrumentline"
chmod 644 "$shim_home/bin/instrumentline"
assert_has "ignores a non-executable binary" "$(shim)" "★"
rm -f "$shim_home/bin/instrumentline"
```

- [ ] **Step 2: Run them and verify they fail**

Run: `bash tests/smoke-claude.sh 2>&1 | grep -A6 "statusline shim"`

Expected: FAIL on `falls back to python when no binary is installed`, because `claude/bin/statusline` does not exist yet.

- [ ] **Step 3: Write the shim**

Create `claude/bin/statusline`:

```bash
#!/usr/bin/env bash
# Chooses the status line at render time. install.sh builds and links
# instrumentline; on a machine with no Rust toolchain that link is absent and
# the python line runs instead. Deciding here rather than in settings.json
# matters because settings.json is a symlink into the repo, so an installer
# that rewrote it per machine would dirty version control.
set -u

home="${CLAUDE_HOME:-$HOME/.claude}"

# exec, so the payload already on stdin passes straight through.
[ -x "$home/bin/instrumentline" ] && exec "$home/bin/instrumentline" render
exec python3 "$home/bin/statusline.py"
```

Then: `chmod 755 claude/bin/statusline`

- [ ] **Step 4: Run the assertions and verify they pass**

Run: `bash tests/smoke-claude.sh 2>&1 | grep -A6 "statusline shim"`

Expected: every assertion `ok`. The instrumentline branch may print `skip` if the crate has not been built in this checkout; that is acceptable.

- [ ] **Step 5: Point settings.json at the shim**

In `claude/settings.json`, replace the `statusLine` block with:

```json
"statusLine": {
  "type": "command",
  "command": "~/.claude/bin/statusline",
  "padding": 0,
  "refreshInterval": 1
}
```

`refreshInterval` drops from 5 to 1 because instrumentline's eased transitions and shimmer are built around a 1Hz repaint. `padding` drops from 1 to 0 so the three rows sit flush.

- [ ] **Step 6: Confirm the odd-refreshInterval assertion still holds**

The suite asserts `refreshInterval % 2 == 1` to keep the Python line's alert pulse alternating while idle. `1 % 2 == 1`, so it passes. Add a clarifying comment above that assertion, since the setting now only affects the fallback path:

```bash
# Guards the python fallback's pulse phase: on an even interval, idle repaints
# always land on the same phase and the alert never flashes. instrumentline
# does not use this, but the fallback still does.
```

- [ ] **Step 7: Add the shim to the installer manifest**

In `install.sh:63`, add `bin/statusline` to the claude tree's `MANAGED_FILES`:

```bash
      MANAGED_FILES=(settings.json CLAUDE.md model-rates.json bin/statusline bin/statusline.py)
```

- [ ] **Step 8: Regenerate the frozen manifest string and verify**

```bash
./install.sh --tree claude --print-manifest | cut -f2 | sort | tr '\n' ' '
```

Paste the result into the assertion, then run: `bash tests/smoke-claude.sh`

Expected: `all claude smoke tests passed`. The `settings.json wiring` group must now report **9** referenced scripts and confirm all are executable — `bin/statusline` is among them, which is why Step 3's `chmod 755` matters.

- [ ] **Step 9: Commit**

```bash
git add claude/bin/statusline claude/settings.json install.sh tests/smoke-claude.sh
git commit -m "feat(claude): select the status line through a shim

settings.json is a symlink into this repo, so an installer that rewrote
statusLine per machine would dirty version control. The shim decides at
render time instead: instrumentline when it is linked, statusline.py
otherwise. Nothing links the binary yet, so this is a no-op for now.

refreshInterval 5 -> 1 and padding 1 -> 0 for instrumentline's animation."
```

---

### Task 4: Teach the installer to build and link the binary

`status_of()` and `manifest()` both assume `dest_rel == src_rel`. The binary breaks that: it lives at `instrumentline/target/release/instrumentline` but installs to `bin/instrumentline`.

**Three call sites consume the source path, not one.** An earlier draft of the design patched only `status_of()`; the install loop's `ln -sfn "$SRC/$rel"` at line 215 would then have created `~/.claude/bin/instrumentline -> claude/bin/instrumentline`, which does not exist. `[ -x ]` on a dangling symlink is false, so the shim would fall back to Python permanently while the installer reported success. Route every consumer through one resolver.

**Files:**
- Modify: `install.sh`
- Modify: `tests/smoke-claude.sh`
- Commit: the whole `claude/instrumentline/` crate

**Interfaces:**
- Consumes: Task 3's shim, which looks for `$CLAUDE_HOME/bin/instrumentline`.
- Produces: `source_of(dest_rel) -> src_rel`; the `MANAGED_BUILT` array; a `--skip-build` flag.

- [ ] **Step 1: Add the resolver and the array**

In `install.sh`, add `MANAGED_BUILT` to both branches of `select_tree()`. Claude (after line 64):

```bash
      # dest=src. The binary is built into target/ but installs to bin/.
      MANAGED_BUILT=(bin/instrumentline=instrumentline/target/release/instrumentline)
```

Copilot (after line 81) — **required, not optional.** `select_tree()` only assigns and never resets, so without this a stale claude value leaks across and the installer tries to link `~/.copilot/bin/instrumentline`:

```bash
      MANAGED_BUILT=()
```

Then add the resolver immediately after `select_tree()` closes at line 85:

```bash
# dest_rel -> src_rel. Identity unless MANAGED_BUILT remaps it. Every consumer
# of a source path must go through this: status_of, the --check loop and the
# install loop's ln. A missed one silently links a dangling symlink.
source_of() {
  local rel="$1" pair
  for pair in ${MANAGED_BUILT[@]+"${MANAGED_BUILT[@]}"}; do
    if [ "${pair%%=*}" = "$rel" ]; then
      printf '%s\n' "${pair#*=}"
      return
    fi
  done
  printf '%s\n' "$rel"
}
```

The `${MANAGED_BUILT[@]+"${MANAGED_BUILT[@]}"}` guard is required: under bash 3.2 with `set -u`, expanding an empty array bare is an unbound-variable error. The file already does this for `LEGACY_HOOKS`.

- [ ] **Step 2: Emit the built paths from manifest()**

Inside `manifest()`, after the `for dir in "${MANAGED_DIRS[@]}"` loop closes (line 103) and before the closing brace:

```bash
  local pair
  for pair in ${MANAGED_BUILT[@]+"${MANAGED_BUILT[@]}"}; do
    printf '%s\n' "${pair%%=*}"
  done
```

Emit unconditionally, even when the binary has not been built. The manifest describes what the installer manages, not what happens to exist, and a build-dependent manifest would make the frozen assertion flap.

- [ ] **Step 3: Resolve the source in status_of()**

Change line 108 from:

```bash
  local rel="$1" link="$DEST/$1" want="$SRC/$1"
```

to:

```bash
  local rel="$1" link="$DEST/$1" want="$SRC/$(source_of "$1")"
```

This one edit covers both the `--check` loop (line 134) and the install loop's status call (line 207), since both call `status_of`.

- [ ] **Step 4: Resolve the source at the link site, and skip when unbuilt**

Replace lines 212-217 of the install loop:

```bash
    # A broken or foreign symlink has nothing worth keeping; a real file does.
    [ "$status" = replaced ] && preserve "$rel"
    src_rel="$(source_of "$rel")"
    # Linking a source that is not there yields a dangling symlink, which the
    # shim reads as "not installed" but --check reports as broken. Skip instead.
    if [ ! -e "$SRC/$src_rel" ]; then
      printf '  %sskip%s  %s %s(not built)%s\n' "$YELLOW" "$RESET" "$rel" "$GRAY" "$RESET"
      continue
    fi
    mkdir -p "$DEST/$(dirname "$rel")"
    ln -sfn "$SRC/$src_rel" "$DEST/$rel"
    printf '  %slink%s  %s\n' "$GREEN" "$RESET" "$rel"
    linked=$((linked + 1))
```

- [ ] **Step 5: Add the --skip-build flag**

Next to `skip_tests=0` at line 20, add `skip_build=0`. In the argument `case` next to `--skip-tests` at line 26, add:

```bash
    --skip-build) skip_build=1 ;;
```

Extend the help text (the `sed -n '2,12p'` range at the top of the file) with a line documenting it, and widen the range to `'2,13p'` so the new line prints.

- [ ] **Step 6: Build before the smoke suite**

Insert between line 174 (the `python3` check) and line 176 (`if [ "$skip_tests" -eq 0 ]`). It must come before the suite, because the suite asserts on the built binary:

```bash
if [ "$skip_build" -eq 0 ] && printf '%s\n' "${TREES[@]}" | grep -qx claude; then
  if command -v cargo >/dev/null 2>&1; then
    printf '%sbuilding instrumentline%s\n' "$BOLD" "$RESET"
    (cd "$REPO/claude/instrumentline" && cargo build --release) || {
      printf '%sbuild failed; the status line will fall back to python%s\n' "$YELLOW" "$RESET" >&2
    }
    printf '\n'
  else
    printf '%scargo not found; the status line will fall back to python%s\n' "$YELLOW" "$RESET" >&2
  fi
fi
```

A missing or failing `cargo` is a **warning, not an error**. Install proceeds, Step 4 skips the unbuilt link, and the shim falls back. Note the subshell around `cd` so the working directory does not leak.

- [ ] **Step 7: Assert the installed path, not just the source tree**

Rendering the binary out of `target/release/` proves the binary works; it does not prove the installer wired it up. The dangling-symlink bug passes every source-tree assertion. Append to `tests/smoke-claude.sh` before the `installer manifest` group:

```bash
# --------------------------------------------------------------------------
group "installer built-artifact resolution"

# A throwaway DEST, so nothing here touches the real ~/.claude.
fake_dest="$TMP/fakeclaude"
mkdir -p "$fake_dest"
CLAUDE_HOME="$fake_dest" bash "$REPO/install.sh" --tree claude --skip-tests --skip-build >/dev/null 2>&1

if [ -x "$REPO/claude/instrumentline/target/release/instrumentline" ]; then
  assert_eq "the installed binary resolves to a real file" \
    "$([ -e "$fake_dest/bin/instrumentline" ] && echo resolves || echo dangling)" "resolves"
  assert_eq "the installed binary is executable" \
    "$([ -x "$fake_dest/bin/instrumentline" ] && echo yes || echo no)" "yes"
  assert_eq "it points at the build output, not bin/" \
    "$(readlink "$fake_dest/bin/instrumentline")" \
    "$REPO/claude/instrumentline/target/release/instrumentline"
else
  assert_eq "an unbuilt binary is skipped, not left dangling" \
    "$([ -L "$fake_dest/bin/instrumentline" ] && echo dangling || echo skipped)" "skipped"
fi

assert_eq "the shim was installed as a link" \
  "$(readlink "$fake_dest/bin/statusline")" "$CLAUDE/bin/statusline"

# MANAGED_BUILT must not leak across trees via select_tree.
assert_lacks "the copilot manifest has no binary" \
  "$(bash "$REPO/install.sh" --tree copilot --print-manifest | cut -f2)" "instrumentline"
```

- [ ] **Step 8: Run the assertions**

Run: `bash tests/smoke-claude.sh 2>&1 | grep -A6 "built-artifact"`

Expected: all `ok`. **If `it points at the build output, not bin/` fails, the dangling-symlink bug is present** — recheck Step 4.

- [ ] **Step 9: Verify bash 3.2 array safety**

Run: `bash -u -c 'MANAGED_BUILT=(); for p in ${MANAGED_BUILT[@]+"${MANAGED_BUILT[@]}"}; do echo "$p"; done; echo guarded-ok'`

Expected: `guarded-ok`, no `unbound variable` error.

- [ ] **Step 10: Build, then verify the full gate**

```bash
(cd claude/instrumentline && cargo build --release)
./install.sh --tree claude --print-manifest | cut -f2 | sort | tr '\n' ' '
```

Paste the result into the frozen assertion — it now includes `bin/instrumentline`. Then:

```bash
bash tests/smoke.sh
```

Expected: both trees green.

- [ ] **Step 11: Commit the installer and the crate**

`target/` is gitignored, so this commits source only. Confirm before committing:

```bash
git add claude/instrumentline install.sh tests/smoke-claude.sh
git status --short | grep -c 'target/'
```

Expected: `0`. Then:

```bash
git commit -m "feat(claude): build and link instrumentline from install.sh

The binary is built into target/ but installs to bin/, which the
installer's dest==src assumption could not express. source_of() resolves
the difference for all three consumers: status_of, the --check loop and
the ln call. Patching only status_of would have linked a dangling
symlink and silently disabled the binary.

A missing cargo is a warning, not an error; the shim falls back."
```

---

### Task 5: Add the instrumentline rendering smoke group

**Files:**
- Modify: `tests/smoke-claude.sh`

**Interfaces:**
- Consumes: the built binary from Task 4.
- Produces: coverage that the binary degrades rather than blanking the bar.

- [ ] **Step 1: Add the group**

Append to `tests/smoke-claude.sh` before the `installer manifest` group. The binary has no `NO_COLOR` support, so strip ANSI locally — do not copy the `NO_COLOR=1` pattern used above:

```bash
# --------------------------------------------------------------------------
group "instrumentline"

IL="$REPO/claude/instrumentline/target/release/instrumentline"
if [ ! -x "$IL" ]; then
  printf '  \033[33mskip\033[0m  not built (run cargo build --release)\n'
else
  ESC="$(printf '\033')"
  strip_ansi() { sed "s/${ESC}\[[0-9;]*m//g"; }
  # An isolated state dir: the binary persists eased values per session.
  il() { INSTRUMENTLINE_STATE_DIR="$TMP/ilstate" "$IL" render; }

  for fixture in "$REPO"/claude/instrumentline/tests/fixtures/*.json; do
    name="$(basename "$fixture")"
    out="$(il <"$fixture")"
    assert_eq "$name exits 0" "$?" "0"
    assert_eq "$name renders three rows" "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" "3"
  done

  # A status line that dies leaves a blank bar, so every failure path must render.
  out="$(printf '{}' | il)"
  assert_eq "empty object exits 0" "$?" "0"
  assert_eq "empty object still renders three rows" "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" "3"
  out="$(printf '{"model":{' | il)"
  assert_eq "malformed json exits 0" "$?" "0"
  assert_eq "malformed json still renders three rows" "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" "3"

  # Bare invocation must behave as `render`, since the shim relies on the subcommand.
  out="$(printf '{}' | INSTRUMENTLINE_STATE_DIR="$TMP/ilstate" "$IL")"
  assert_eq "bare invocation defaults to render" "$(printf '%s\n' "$out" | wc -l | tr -d ' ')" "3"

  assert_has "doctor reports a writable state dir" \
    "$(INSTRUMENTLINE_STATE_DIR="$TMP/ilstate" "$IL" doctor | strip_ansi)" "state writable  yes"
fi
```

- [ ] **Step 2: Run the group**

Run: `bash tests/smoke-claude.sh 2>&1 | grep -A24 '^.\[1minstrumentline'`

Expected: every assertion `ok`, covering all six fixtures.

- [ ] **Step 3: Verify the whole suite**

Run: `bash tests/smoke.sh`

Expected: both trees green.

- [ ] **Step 4: Commit**

```bash
git add tests/smoke-claude.sh
git commit -m "test(claude): cover instrumentline rendering and degradation

Assertions strip ANSI themselves; the binary has no NO_COLOR support, so
the pattern the python assertions use does not transfer."
```

---

### Task 6: Add the glyph spacing, and measure what it costs

Five sites put a glyph directly against its number, so `⚒41` reads as one token.

**Do not verify this by checking for overflow.** `fit_to_width` (`src/render/layout.rs:29-46`) drops priority groups until the line fits, then calls `truncated_to(columns)`. A row cannot overflow by construction, and the existing `no_row_ever_exceeds_the_column_budget` test asserts that tautology. The real failure is silent content loss, already live: at `COLUMNS=80`, `critical.json` row 3 ends mid-word at `cached - con`.

**Files:**
- Modify: `claude/instrumentline/src/render/widgets.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a report of content lost at 80 and 100 columns, for the user to act on separately.

- [ ] **Step 1: Capture the before state**

```bash
(
  cd claude/instrumentline
  ESC=$(printf '\033')
  for w in 80 100; do
    for f in tests/fixtures/*.json; do
      printf '=== %s @ %s ===\n' "$(basename "$f")" "$w"
      COLUMNS=$w INSTRUMENTLINE_STATE_DIR=/tmp/ilbefore ./target/release/instrumentline render <"$f" \
        | sed "s/${ESC}\[[0-9;]*m//g"
    done
  done
) > /tmp/glyph-before.txt
wc -l /tmp/glyph-before.txt
```

Expected: a non-empty file. Keep it; Step 5 diffs against it.

- [ ] **Step 2: Apply all five edits**

In `src/render/widgets.rs`:

| Line | From | To |
| --- | --- | --- |
| 272 | `glyphs.tool_calls.clone(),` | `format!("{} ", glyphs.tool_calls),` |
| 280 | `format!(" {}", glyphs.turns),` | `format!(" {} ", glyphs.turns),` |
| 289 | `format!(" {}", glyphs.subagents),` | `format!(" {} ", glyphs.subagents),` |
| 304 | `format!(" {}", glyphs.errors),` | `format!(" {} ", glyphs.errors),` |
| 369 | `format!(" {}{countdown}", glyphs.reset_clock),` | `format!(" {} {countdown}", glyphs.reset_clock),` |

Line 272 changes from a `clone()` to a `format!`, which is why it looks unlike the other four.

- [ ] **Step 3: Rebuild**

Run: `(cd claude/instrumentline && cargo build --release) 2>&1 | tail -5`

Expected: `Finished`, no warnings. Clippy denies `unwrap`/`panic` outside tests but none of these edits introduce any.

- [ ] **Step 4: Verify the spacing landed**

```bash
(cd claude/instrumentline && INSTRUMENTLINE_STATE_DIR=/tmp/ilafter \
  ./target/release/instrumentline render < tests/fixtures/longhaul.json) \
  | sed "s/$(printf '\033')\[[0-9;]*m//g" | sed -n 2p
```

Expected: counters read `⚒ N ↻ N`, with a space after each glyph.

- [ ] **Step 5: Capture the after state and diff**

```bash
(
  cd claude/instrumentline
  ESC=$(printf '\033')
  for w in 80 100; do
    for f in tests/fixtures/*.json; do
      printf '=== %s @ %s ===\n' "$(basename "$f")" "$w"
      COLUMNS=$w INSTRUMENTLINE_STATE_DIR=/tmp/ilafter ./target/release/instrumentline render <"$f" \
        | sed "s/${ESC}\[[0-9;]*m//g"
    done
  done
) > /tmp/glyph-after.txt
diff /tmp/glyph-before.txt /tmp/glyph-after.txt
```

- [ ] **Step 6: Report the cost, do not compensate for it**

Read the diff. For each row that changed, state whether it gained only spaces or **lost content** — a truncated word, or a whole group (spend, cache hit, burn rate, alert lane) that vanished.

Report the findings to the user. **Do not redesign the layout, shorten labels, or trim other content.** The user decides separately once the cost is visible. If a whole group disappears at 80 columns, say which one explicitly.

- [ ] **Step 7: Run the crate's own tests**

Run: `(cd claude/instrumentline && scripts/test.sh) 2>&1 | tail -20`

Expected: all green. No test asserts on rendered text, so the spacing change should not break any. **If one fails, stop and report** rather than editing the test to match.

- [ ] **Step 8: Verify the smoke suite still passes**

Run: `bash tests/smoke.sh`

Expected: both trees green.

- [ ] **Step 9: Commit**

```bash
git add claude/instrumentline/src/render/widgets.rs
git commit -m "style(instrumentline): space every glyph from its value

The activity counters and the reset clock ran their glyph straight into
the number, so the pair read as one token."
```

---

### Task 7: Fix the README

Two separate problems in two separate subsections.

**Files:**
- Modify: `claude/instrumentline/README.md`

- [ ] **Step 1: Fix the missing dot in all three places**

The path is `~/.claude`, not `~/claude`. Anyone following this verbatim created a stray `~/claude/` directory and a hook that never fired, so the status line would have shown the default permission mode forever.

| Line | Context |
| --- | --- |
| 49 | the `install -Dm755` command |
| 64 | the `UserPromptSubmit` hook command |
| 67 | the `PreToolUse` hook command |

- [ ] **Step 2: Replace the install instructions**

The `## Install` section's `cargo build` plus `install -Dm755` block is superseded. Replace the whole code block with:

```sh
# From the repository root. Builds the crate, links it into ~/.claude,
# and falls back to the python status line if cargo is unavailable.
./install.sh --tree claude
```

- [ ] **Step 3: Correct the settings.json block**

Update it to the wiring this plan actually produces — the shim, not the binary directly:

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

Add a sentence noting that `install.sh` writes none of this — it is what the repo's `claude/settings.json` already contains, shown for anyone using the crate standalone.

- [ ] **Step 4: Verify no missing-dot path survives**

Run: `grep -rn '~/claude\|\$HOME/claude' --exclude-dir=target claude/`

Expected: no output. Scoped to `claude/` deliberately — the unscoped version matches the design document several times and could never pass.

- [ ] **Step 5: Verify the hook filename is current**

Run: `grep -n 'write-permission-mode' claude/instrumentline/README.md`

Expected: no output. The file was renamed to underscores in Task 2.

- [ ] **Step 6: Commit**

```bash
git add claude/instrumentline/README.md
git commit -m "docs(instrumentline): fix the install path and instructions

The hook path was missing its dot in three places, so following the
README produced a stray ~/claude directory and a hook that never fired."
```

---

### Task 8: Install for real and restore the settings symlink

**Do not start this task until Tasks 1-7 are committed and green.** This is the only task whose effects reach the running Claude Code session.

**Files:**
- Modify: `~/.claude/settings.json` (restores a symlink; no repo file changes)

- [ ] **Step 1: Confirm the full gate is green**

```bash
(cd claude/instrumentline && scripts/check.sh); echo "check=$?"
bash tests/smoke.sh; echo "smoke=$?"
```

Expected: both `0`. **If `scripts/check.sh` fails on a clippy or format nit introduced by Task 6, fix it. If it fails on something pre-existing and unrelated, stop and report** rather than fixing it here.

- [ ] **Step 2: Confirm the settings file is still detached**

```bash
ls -l ~/.claude/settings.json
```

Expected: a regular file, **not** a symlink. If it is already a symlink, something restored it early — stop and report.

- [ ] **Step 3: Diff the live settings against the repo**

```bash
diff <(python3 -m json.tool ~/.claude/settings.json) \
     <(python3 -m json.tool claude/settings.json)
```

Expected: differences confined to `statusLine` and `hooks` — precisely the changes from Tasks 2 and 3. **Any other difference means the live file drifted while detached; report it before overwriting.**

- [ ] **Step 4: Install**

```bash
./install.sh --tree claude
```

Expected: the smoke suite passes, then a list of linked paths. `settings.json` should appear as `link`, and the real file is copied into `~/.claude/backups/agentprofiles-<stamp>/` first. `bin/instrumentline` should appear as `link`, not `skip`.

- [ ] **Step 5: Verify every link resolves**

```bash
./install.sh --check
ls -l ~/.claude/settings.json ~/.claude/bin/statusline ~/.claude/bin/instrumentline
```

Expected: `everything is linked`. All three are symlinks into the repo, and `bin/instrumentline` resolves to `target/release/instrumentline`.

- [ ] **Step 6: Verify the shim picks the binary end to end**

```bash
~/.claude/bin/statusline < tests/fixtures/statusline-full.json | wc -l
~/.claude/bin/statusline < tests/fixtures/statusline-full.json | grep -c '★' || true
```

Expected: `3` rows, and `0` verdict glyphs — `★` would mean the Python line ran, so the binary is not being selected.

- [ ] **Step 7: Confirm in the live session**

Ask the user to look at their status line. It should be three rows with the mode rail on the left, updating about once a second.

**If the bar is blank, recover immediately:**

```bash
rm ~/.claude/bin/instrumentline   # the shim falls straight back to python
```

Then report what happened. Do not debug against a broken live status line.

- [ ] **Step 8: Verify the permission-mode hook fires**

Ask the user to switch permission mode (shift-tab), then:

```bash
ls -l ~/.claude/instrumentline-state/*.mode | tail -3
cat ~/.claude/instrumentline-state/*.mode | tail -1
```

Expected: at least one `.mode` file, recently modified, containing the current mode. The mode badge in row 1 should match.

- [ ] **Step 9: Final state check**

```bash
git status --short
git log --oneline main..HEAD
```

Expected: a clean tree and seven commits. Nothing under `target/` is tracked.

---

## Post-Implementation

Report to the user:

1. The width diff from Task 6 Step 6 — exactly what content is lost at 80 columns, and whether the spacing made it worse.
2. That the installer now links `agents/ci-watcher.md`, `bin/hooks/capper_simulate.py` and `bin/hooks/edit_capper.py`, absorbed from the pre-existing drift in Task 1.
3. The deferred items from the spec, unchanged: state-directory growth with no pruning, unreachable `GlyphTable::ascii_fallback()`, no `NO_COLOR` support.

Then invoke **superpowers:finishing-a-development-branch** to decide how `feat/instrumentline` gets integrated.
