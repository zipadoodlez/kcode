# Spawn Hook: External Control of Headed Session Spawns

jcode opens new terminal windows in several flows: swarm agent spawning
(`swarm spawn` with `spawn_mode=visible`), resume-in-new-terminal, self-dev
sessions, restart restores, and jade relay launches. By default jcode detects
an installed terminal emulator (kitty, wezterm, alacritty, gnome-terminal, ...)
and opens a new OS window.

The **spawn hook** lets an external program take over this spawn so it can
decide *where and how* the session appears: a tmux pane, a kitty tab, a zellij
pane, a tab in a wrapper app like herd, a specific monitor/workspace, etc.

## Configuration

```toml
# ~/.jcode/config.toml
[terminal]
spawn_hook = "tmux new-window"
```

Or per-environment:

```bash
export JCODE_SPAWN_HOOK="tmux new-window"
# An empty value disables a config-file hook:
export JCODE_SPAWN_HOOK=
```

Env always wins over the config file.

## Contract

When a headed spawn happens and a hook is configured, jcode runs:

```
<spawn_hook> <jcode-binary> <args...>
```

- The hook command is parsed shell-style (quotes and backslash escapes work),
  but it is executed directly, not through a shell.
- The jcode binary and its full argument list are appended as extra argv
  entries (the familiar `$TERMINAL -e <cmd>` convention).
- The hook's working directory is the session working directory.
- The hook process is detached; jcode does not wait for it.
- If the hook fails to start (binary missing, parse error), jcode logs a
  warning and falls back to its built-in terminal detection.

### Metadata environment

The hook (and any terminal spawned by the built-in fallback) receives:

| Variable | Meaning |
| --- | --- |
| `JCODE_SPAWN_KIND` | Why the spawn happened: `swarm-agent`, `resume`, `selfdev`, `restart`, `jade-relay` |
| `JCODE_SPAWN_SESSION_ID` | The jcode session the window will run |
| `JCODE_SPAWN_TITLE` | Suggested window/tab title (includes session icon + name) |
| `JCODE_SPAWN_CWD` | Session working directory |
| `JCODE_SPAWN_PROGRAM` | Path of the jcode binary to execute |
| `JCODE_SPAWN_COMMAND` | Full command line, shell-escaped, for hooks that take one shell string |
| `JCODE_SPAWN_SWARM_ID` | (swarm spawns) The swarm the agent joins |
| `JCODE_SPAWN_COORDINATOR_SESSION_ID` | (swarm spawns) The coordinator session that requested the spawn |
| `JCODE_FRESH_SPAWN` | `1` when the spawn is a fresh window handoff |

## Examples

### tmux: one window per agent

```toml
[terminal]
spawn_hook = "tmux new-window"
```

`tmux new-window <jcode> --resume ses_x` runs the command in a new window of
the current tmux server. For panes instead:

```toml
[terminal]
spawn_hook = "tmux split-window -h"
```

### kitty: one tab per agent (remote control)

```toml
[terminal]
spawn_hook = "kitty @ --to unix:/tmp/kitty.sock launch --type=tab --"
```

### Custom router script

For full control (placement, titles, swarm vs resume routing), point the hook
at a script:

```toml
[terminal]
spawn_hook = "~/bin/jcode-spawn-router"
```

```bash
#!/usr/bin/env bash
# ~/bin/jcode-spawn-router
# argv: the jcode command to run ("$@"). Env: JCODE_SPAWN_* metadata.

case "$JCODE_SPAWN_KIND" in
  swarm-agent)
    # Swarm workers as tmux panes in a window named after the swarm.
    tmux new-window -n "swarm:${JCODE_SPAWN_SWARM_ID:0:8}" "$@" 2>/dev/null \
      || tmux split-window "$@"
    ;;
  *)
    # Everything else as a normal terminal window.
    kitty --title "$JCODE_SPAWN_TITLE" -e "$@" &
    ;;
esac
```

A hook that exits non-zero after launching nothing will NOT trigger the
built-in fallback (jcode only falls back when the hook process cannot be
started), so a router script should handle its own fallback like the example
above.

### Single-shell-string consumers

Some launchers want one shell command string instead of argv. Use
`$JCODE_SPAWN_COMMAND`:

```bash
#!/usr/bin/env bash
zellij action new-pane -- bash -lc "$JCODE_SPAWN_COMMAND"
```

## Programmatic discovery

Programs that wrap jcode (e.g. herd-style session managers) can set
`JCODE_SPAWN_HOOK` in the environment of the `jcode` server process they
launch. Every headed spawn the server performs, including swarm agents
requested by coordinators over the socket protocol, will then route through
the wrapper's hook.
