# agentprofiles

Agent harness configuration for two tools, kept in one place so it can be versioned and reinstalled.

- `claude/` — settings, hooks, statusline, skills, and agents for [Claude Code](https://code.claude.com)
- `copilot/` — the equivalent for GitHub Copilot CLI
- `ideas/` — experiments, not installed by anything

## Installing

```sh
./install.sh
```

This runs the smoke suite first and refuses to touch anything if it fails. Then it backs up whatever it is about to replace and symlinks both trees onto their config directories — `claude/` onto `~/.claude`, `copilot/` onto `~/.copilot`. Because they are symlinks, editing a file here takes effect immediately — there is no second step.

```sh
./install.sh --tree claude      # one tree only (also: copilot, all)
./install.sh --check            # audit what is linked, without writing
./install.sh --print-manifest   # list the managed paths and exit
./install.sh --skip-tests       # install without running the smoke suite
```

`--tree` also narrows the smoke gate, so a failure on one side cannot block installing the other.

Set `CLAUDE_HOME` or `COPILOT_HOME` to install somewhere other than the defaults, which is how the installer is tested.

### What it links

`settings.json`, `CLAUDE.md`, `model-rates.json`, and `bin/statusline.py` are linked directly. `bin/hooks/`, `skills/`, and `agents/` are linked one entry at a time rather than as whole directories.

That distinction matters. Linking the directories would hide anything that exists only in `~/.claude` — skills installed by a plugin marketplace, agents added by hand. Per-entry linking installs what the repo carries and leaves the rest alone. Where both sides have a file of the same name, the repo version wins and the previous one goes into the backup.

On the Copilot side the same rule applies to `hooks/`, `hooks/bin/`, `skills/`, and `agents/`, with `settings.json`, `model-rates.json`, and `statusline.py` linked directly. Those four directories are named exactly and the config-directory root is never enumerated: `~/.copilot` is a live product directory holding `session-store.db`, `session-state/`, and `logs/`, none of which the installer may go near. `copilot/copilot-instructions.md` is deliberately unmanaged while it is empty, since linking it over a real one would blank your instructions.

Backups land in `<config dir>/backups/agentprofiles-<timestamp>/`, laid out the same way as the config directory itself. To undo something, delete the symlink and copy the file back.

### Retired hooks

The installer removes four hooks from `~/.claude/hooks/` — `notify.sh`, `protected-paths-guard.sh`, `ruff-on-edit.sh`, and `session-context.sh`. They are superseded by the underscored versions in `claude/bin/hooks/`, which is where `settings.json` actually points. They are backed up before removal.

### One caveat

`settings.json` is a symlink, and Claude Code rewrites that file when you change something through `/config`. Depending on how it writes, the symlink may be silently replaced by a regular file, at which point the repo is no longer the source of truth. `./install.sh --check` reports this as `replaced`. Run it after changing settings through the UI.

## Statusline

`claude/bin/statusline.py` renders three lines:

```
★ Opus [high] ✻  ·  agentprofiles  ·  main ~1 ?1  ·  v2.1.214
◉ [#####----·····] 41%  ◌ 82k/200k  58k left  ·  ⟳ turns 12  ·  ⌕ tools 47
5h [#-------] 22% 8:53pm  ·  7d [####----] 61% 12:00pm  ·  +156/-23  ·  $0.31  ·  ~$0.14/msg  ·  $1.55/hr  ·  cache 83%  ·  12m
```

Line two is what the session is consuming right now, line three is what it has spent. A fresh session with no recorded limits and nothing billed yet prints only the first two.

The glyph and colour on the model name come from `claude/model-rates.json`, which also prices `~$/msg` — what it costs to send the current context once, at the current model's rates.

In the context bar, the yellow `·` marks where auto-compaction triggers and the dots past it are window the session never reaches, since compaction fires first. The activity glyphs match the Copilot statusline: `⟳` turns, `⌕` tools, `⌬` agents, `✕` errors.

`~$/msg` is a **lower bound**. Cache writes bill at 1.25× base input for a five-minute TTL and 2× for an hour, and the statusline payload reports a single `cache_creation_input_tokens` count that does not say which was used. The figure prices the cheaper one. When the gap between the two clears ten cents, a banner appears saying what those writes cost at the hourly rate:

```
(!) CACHE_WRITE 200k · ~$2.00 if 1h TTL (!)
```

Red and bold, and it flashes: the underline toggles on alternate seconds. That is done with SGR 4 rather than the blink attribute SGR 5, which Windows Terminal ignores and which no terminal implements for SGR 6. `CLAUDE_STATUS_NO_ALERT=1` suppresses the banner.

The flash depends on `statusLine.refreshInterval` in `settings.json`. Updates are otherwise event-driven, so without a timer the banner would freeze mid-phase whenever the session went idle — which is exactly when it needs to be noticed. The interval must also be an **odd** number of seconds, or every idle repaint lands on the same phase and the underline never changes. It is currently `5`; a smoke assertion holds it odd. Set it to `1` for a brisk one-hertz pulse at the cost of running the script five times as often.

Nothing keys off `exceeds_200k_tokens`. Claude Code reports it as a fixed threshold with no billing meaning, and Claude 4.6 and later carry the full 1M-token window at standard rates, so there is no long-context premium to warn about. An earlier version of this statusline claimed otherwise.

Rate limit windows are recorded to `~/.cache/claude-statusline/rate-limits.json` and replayed, because Claude Code only includes them in some payloads. A recorded window is discarded once its reset time passes.

Two environment variables adjust it: `NO_COLOR=1` strips escape sequences, and `CLAUDE_STATUS_NO_ALERT=1` suppresses the cache-write banner.

## Copilot statusline

`copilot/statusline.py` renders the same three lines against Copilot's payload:

```
★ (gpt-5.6-luna) [high]  ·  agentprofiles  ·  main ~1 ?1  ·  v0.0.42
◉ [#####----] 41%  ◌ 82k/200k  ·  ⌕ tools(47) ◦ ⟳ turns(12) ◦ ✕ errors(0) ◦ ⌬ agents(2)
$0.69  ·  31 aic  ·  ~$0.14/msg  ·  $3.45/hr  ·  cache 83%  ·  12:00
```

There is no quota gauge. The plan this was written for is enterprise with no usage cap, so a percentage-of-quota bar would be measuring against nothing; line three carries absolute figures instead.

Two numbers, because they come from different places. The dollar figure is our own arithmetic over the payload's cumulative token counters priced through `copilot/model-rates.json`. The `aic` figure is GitHub's own, read from `ai_used` — one AI credit is one US cent. When the two disagree, GitHub's is the one that gets billed.

`~$/msg` is a **lower bound**, as on the Claude side. It prices resending the current context once, from `last_call_input_tokens` — not the cumulative `total_input_tokens`, which grows all session and would inflate the figure without bound. It charges nothing for the output the turn will produce, and nothing for the cache writes it may trigger. The cache-write banner and its alternating underline work exactly as described above, suppressed by `COPILOT_STATUS_NO_ALERT=1`. Only the Anthropic models bill cache writes at all, so on a GPT or Gemini model it never fires.

An unpriced model prints its token volume and says so rather than inventing a rate:

```
125k tok, no rate  ·  cache 0%
```

Billing is priced from the statusline payload alone. `~/.copilot/session-store.db` does hold cost data, but GitHub publishes no schema for it, marks the file automatically managed, and warns that it changes between releases — so nothing here reads it.

The two rate tables key differently on purpose: `claude/model-rates.json` uses API model IDs (`claude-opus-4-8`), `copilot/model-rates.json` uses Copilot's display names (`claude-opus-4.8`). Only the schema is shared. Their `q_score` columns come from different scales and are not comparable across the two files.

## Copilot hooks

Ported from the Claude side, adapted to Copilot's contract:

| Event | Script | What it does |
|---|---|---|
| `preToolUse` | `protected_paths_guard.sh` | denies credential paths, asks about environment dumps and curl-pipe-shell |
| `sessionStart` + `userPromptTransformed` | `session_context.py` | captures repo state, injects it into the first prompt |
| `postToolUse` | `read_logger.py` | records what was read |
| `notification` | `notify.sh` | desktop notification, terminal bell as fallback |

Two contract differences from Claude are load-bearing. Copilot's verdict is flat — `{"permissionDecision": ..., "permissionDecisionReason": ...}` — with no `hookSpecificOutput` wrapper. And `preToolUse` treats a **non-zero exit as a deny** regardless of what stdout said, so the guard exits 0 on every path including when it fails closed; a guard that ever crashed would block every tool call for the rest of the session.

The credential deny list lives inside the guard rather than in `settings.json`. Claude expresses it as 25 `permissions.deny` entries; Copilot's `permissions` object supports only `disableBypassPermissionsMode`, so the guard has to see every tool call. That is roughly one extra process spawn per tool.

`ruff_on_edit.py` is not ported. `ideas/tool-filter.py` is a sketch, not an installed hook.

## Tests

```sh
bash tests/smoke.sh                  # both trees
SMOKE_TREE=copilot bash tests/smoke.sh
```

Covers statusline rendering against JSON fixtures, every hook, cost arithmetic against hand-computed figures, and whether every path the configs reference exists and is executable. No dependencies beyond `python3` and `git`.

**The Copilot fixtures are hand-authored from GitHub's documentation, not captured from a live CLI**, and each says so in a `_source` key. Copilot was not installed on the machine this was written on, so nothing here has been observed executing. If a payload key name turns out to be wrong, every assertion still passes while the real hook silently does nothing. Capturing one live payload per event is the only thing that closes that gap.
