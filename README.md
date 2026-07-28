# agentprofiles

Agent harness configuration for two tools, kept in one place so it can be versioned and reinstalled.

- `claude/` — settings, hooks, statusline, skills, and agents for [Claude Code](https://code.claude.com)
- `copilot/` — the equivalent for GitHub Copilot CLI
- `ideas/` — experiments, not installed by anything

Only the Claude side has an installer today.

## Installing the Claude config

```sh
./install.sh
```

This runs the smoke suite first and refuses to touch anything if it fails. Then it backs up whatever it is about to replace and symlinks the repo's `claude/` tree onto `~/.claude`. Because they are symlinks, editing a file here takes effect immediately — there is no second step.

```sh
./install.sh --check        # audit what is linked, without writing
./install.sh --skip-tests   # install without running the smoke suite
```

Set `CLAUDE_HOME` to install somewhere other than `~/.claude`, which is how the installer is tested.

### What it links

`settings.json`, `CLAUDE.md`, `model-rates.json`, and `bin/statusline.py` are linked directly. `bin/hooks/`, `skills/`, and `agents/` are linked one entry at a time rather than as whole directories.

That distinction matters. Linking the directories would hide anything that exists only in `~/.claude` — skills installed by a plugin marketplace, agents added by hand. Per-entry linking installs what the repo carries and leaves the rest alone. Where both sides have a file of the same name, the repo version wins and the previous one goes into the backup.

Backups land in `~/.claude/backups/agentprofiles-<timestamp>/`, laid out the same way as `~/.claude` itself. To undo something, delete the symlink and copy the file back.

### Retired hooks

The installer removes four hooks from `~/.claude/hooks/` — `notify.sh`, `protected-paths-guard.sh`, `ruff-on-edit.sh`, and `session-context.sh`. They are superseded by the underscored versions in `claude/bin/hooks/`, which is where `settings.json` actually points. They are backed up before removal.

### One caveat

`settings.json` is a symlink, and Claude Code rewrites that file when you change something through `/config`. Depending on how it writes, the symlink may be silently replaced by a regular file, at which point the repo is no longer the source of truth. `./install.sh --check` reports this as `replaced`. Run it after changing settings through the UI.

## Statusline

`claude/bin/statusline.py` renders three lines:

```
★ Opus [high] ✻  ·  agentprofiles  ·  main ~1 ?1
◉ [#####----·----] 41%  ◌ 82k/200k  58k left  ·  5h [#-------] 22% 8:53pm
turns 12  ·  tools 47  ·  +156/-23  ·  $0.31  ·  ~$0.14/msg  ·  $1.55/hr  ·  cache 83%  ·  12m
```

The glyph and colour on the model name come from `claude/model-rates.json`, which also prices `~$/msg` — what it costs to send the current context once, at the current model's rates. The `·` inside the context bar marks where auto-compaction will trigger.

Rate limit windows are recorded to `~/.cache/claude-statusline/rate-limits.json` and replayed, because Claude Code only includes them in some payloads. A recorded window is discarded once its reset time passes.

Two environment variables adjust it: `NO_COLOR=1` strips escape sequences, and `CLAUDE_STATUS_NO_ALERT=1` suppresses the long-context banner.

## Tests

```sh
bash tests/smoke.sh
```

Covers statusline rendering against JSON fixtures, each of the five hooks, and whether every path `settings.json` references actually exists and is executable. No dependencies beyond `python3` and `git`.
