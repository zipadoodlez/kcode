# Hooks

Hooks are how you change what kcode does **without forking it**. You point a
config key at a command of your own; kcode runs it at a well-defined moment and
passes the details as environment variables.

There are two families, and they live in different config sections because they
answer different questions:

| family | section | answers |
|---|---|---|
| lifecycle hooks | `[hooks]` | *what is happening inside a session*, and may it happen |
| terminal hooks | `[terminal]` | *where does a new session window appear*, and how is it focused |

## Which hook do I want?

| I want to... | use |
|---|---|
| get notified when a turn finishes | `turn_end` (observer) |
| know the agent has started working, before any tool call | `turn_start` (observer) |
| log every tool call and how long it took | `post_tool` (observer) |
| **block** a tool call from a policy script | `pre_tool` (gate) |
| track session lifetime (start, resume, close) | `session_start` / `session_end` |
| open agent windows in tmux/zellij/kitty instead of a new OS window | `[terminal] spawn_hook` |
| focus an existing window my way (Wayland, multiplexers) | `[terminal] focus_hook` |

If you are deciding between `pre_tool` and the permission system: **permissions
are how you supervise the agent** (it asks, in-band, and you answer);
**hooks are how you automate your own policy** (a script decides silently).

## Lifecycle hooks

```toml
# ~/.kcode/config.toml
[hooks]
turn_start    = "~/bin/kcode-turn-start"   # observer
turn_end      = "~/bin/kcode-turn-notify"  # observer
session_start = ""                         # observer
session_end   = ""                         # observer
post_tool     = ""                         # observer
pre_tool      = "~/bin/kcode-tool-policy"  # gate
pre_tool_timeout_ms = 5000
```

Every hook has an env override, which always wins. An **empty** env value
disables the config-file hook:

`JCODE_HOOK_TURN_START`, `JCODE_HOOK_TURN_END`, `JCODE_HOOK_SESSION_START`,
`JCODE_HOOK_SESSION_END`, `JCODE_HOOK_PRE_TOOL`, `JCODE_HOOK_POST_TOOL`,
`JCODE_HOOK_PRE_TOOL_TIMEOUT_MS`.

### Common contract

- The command is parsed **shell-style** (quotes and backslash escapes work) but
  executed **directly, not through a shell**. A leading `~/` is expanded.
- It runs with the session working directory as cwd, when known.
- Every hook receives the same base variables:

| variable | meaning |
|---|---|
| `JCODE_HOOK_EVENT` | `turn_start`, `turn_end`, `session_start`, `session_end`, `pre_tool`, `post_tool` |
| `JCODE_HOOK_SESSION_ID` | the session the event belongs to |
| `JCODE_HOOK_CWD` | session working directory |
| `JCODE_HOOK_PAYLOAD` | JSON object mirroring all fields (capped at 16 KB) |
| `JCODE_HOOKS_DISABLED` | always `1`; stops a hook that calls `kcode` from recursing |

Any variable a hook does not set is simply absent, so `"${VAR:-default}"` works.

### Observers

`turn_start`, `turn_end`, `session_start`, `session_end` and `post_tool` are
**observers**: kcode spawns them detached and moves on. They cannot block or
slow the agent, and a failure is only logged.

| hook | when | extra fields |
|---|---|---|
| `turn_start` | a turn begins, after the user message is added and before the model generates. Fires before the first `pre_tool`, so it is the earliest signal that the agent is working | `MODEL`, `SOURCE` |
| `turn_end` | a turn completes (covers TUI, desktop, swarm workers, headless) | `STATUS` (`ok`/`error`), `DURATION_MS`, `MODEL`, `LAST_ASSISTANT_TEXT` (first 4000 chars), `ERROR` (on failure) |
| `session_start` | a session becomes active | `SOURCE` = `create`, `attach` (an existing session object was attached) or `resume` (restored by id) |
| `session_end` | a session closes normally | `SOURCE` = `close` |
| `post_tool` | after every tool call | `TOOL_NAME`, `STATUS` (`ok`/`error`), `DURATION_MS`, `OUTPUT_BYTES` (on success), `ERROR` (on failure) |

### The gate: `pre_tool`

`pre_tool` runs **synchronously before every tool call** and can block it. It is
the only hook that can affect the agent.

- It receives `JCODE_HOOK_TOOL_NAME`, and the full tool input JSON on **stdin**
  (also a 16 KB-truncated copy in `JCODE_HOOK_TOOL_INPUT`).
- **Exit 0** - allow the call.
- **Exit 2** - block the call. The hook's stderr (trimmed, capped at 2000 chars)
  is returned to the model as the tool error, so it can adapt.
- **Anything else fails open** with a logged warning: other exit codes, a
  timeout (`pre_tool_timeout_ms`, default 5000), a missing binary, a spawn error.

Fail-open is deliberate. A broken policy script should degrade to "no policy",
not brick every session. If you need fail-closed, make the hook itself robust -
it is your trust boundary, not kcode's.

### Example: a policy gate

```bash
#!/usr/bin/env bash
# ~/bin/kcode-tool-policy    stdin: tool input JSON
input=$(cat)

case "$JCODE_HOOK_TOOL_NAME" in
  bash)
    if grep -qE 'rm -rf /([^a-zA-Z]|$)|mkfs|dd if=' <<<"$input"; then
      echo "blocked: destructive shell command" >&2   # goes back to the model
      exit 2
    fi
    ;;
  write|edit)
    if grep -q '"file_path":"/etc/' <<<"$input"; then
      echo "blocked: writes to /etc are not allowed" >&2
      exit 2
    fi
    ;;
esac
exit 0
```

### Example: notify + log

```toml
[hooks]
turn_end      = "~/bin/kcode-notify"
session_start = "~/bin/kcode-event-log"
session_end   = "~/bin/kcode-event-log"
post_tool     = "~/bin/kcode-event-log"
```

```bash
#!/usr/bin/env bash
# ~/bin/kcode-notify
if [ "$JCODE_HOOK_STATUS" = ok ]; then icon=ok; else icon=FAILED; fi
notify-send "kcode $icon" "${JCODE_HOOK_LAST_ASSISTANT_TEXT:0:120}"
```

```bash
#!/usr/bin/env bash
# ~/bin/kcode-event-log - one script, fanned out by JCODE_HOOK_EVENT
echo "$JCODE_HOOK_PAYLOAD" >> ~/.local/state/kcode-events.jsonl
```

## Terminal hooks

kcode opens new windows in several flows: visible swarm spawning,
resume-in-new-terminal, self-dev sessions, and restart restores. By default it
detects an installed terminal emulator and opens an OS window. A spawn hook
takes over that decision entirely.

```toml
[terminal]
spawn_hook = "tmux new-window"
focus_hook = "~/bin/kcode-focus"
```

Env overrides: `JCODE_SPAWN_HOOK`, `JCODE_FOCUS_HOOK` (empty disables).

### `spawn_hook`

kcode runs:

```
<spawn_hook> <kcode-binary> <args...>
```

The binary and its full argument list are appended as extra argv entries - the
familiar `$TERMINAL -e <cmd>` convention - so `tmux new-window` and
`kitty --type=tab --` both work unchanged. The hook is detached and not waited
on. If it **cannot be started** (missing binary, parse error), kcode logs a
warning and falls back to built-in terminal detection.

Note the asymmetry: a hook that starts successfully but launches nothing does
**not** trigger the fallback, so a router script should handle its own fallback.

Metadata environment:

| variable | meaning |
|---|---|
| `JCODE_SPAWN_KIND` | why: `swarm-agent`, `resume`, `selfdev`, `restart` |
| `JCODE_SPAWN_SESSION_ID` | the session the window will run |
| `JCODE_SPAWN_TITLE` | suggested window/tab title (session icon + name) |
| `JCODE_SPAWN_CWD` | session working directory |
| `JCODE_SPAWN_PROGRAM` | path of the kcode binary to execute |
| `JCODE_SPAWN_COMMAND` | the full command line, shell-escaped, for launchers that take one shell string |
| `JCODE_SPAWN_SWARM_ID` | (swarm spawns) the swarm the agent joins |
| `JCODE_SPAWN_COORDINATOR_SESSION_ID` | (swarm spawns) the coordinator that requested it |
| `JCODE_FRESH_SPAWN` | `1` when this is a fresh window handoff |

Examples:

```toml
# one tmux window per agent
spawn_hook = "tmux new-window"

# keep the default right-side pane instead
spawn_hook = "tmux split-window -h"

# one kitty tab per agent, via remote control
spawn_hook = "kitty @ --to unix:/tmp/kitty.sock launch --type=tab --"
```

A router script, for placement that depends on why the spawn happened:

```bash
#!/usr/bin/env bash
# ~/bin/kcode-spawn-router     argv: the kcode command to run
case "$JCODE_SPAWN_KIND" in
  swarm-agent)
    tmux new-window -n "swarm:${JCODE_SPAWN_SWARM_ID:0:8}" "$@" 2>/dev/null \
      || tmux split-window "$@"
    ;;
  *)
    kitty --title "$JCODE_SPAWN_TITLE" -e "$@" &
    ;;
esac
```

Launchers that want a single shell string use `$JCODE_SPAWN_COMMAND`:

```bash
#!/usr/bin/env bash
zellij action new-pane -- bash -lc "$JCODE_SPAWN_COMMAND"
```

### `focus_hook`

Bringing an existing session window forward normally uses a best-effort
`wmctrl`/`xdotool` title search, which fails under Wayland and inside
multiplexers. A wrapper that owns placement should own focus too:

```bash
#!/usr/bin/env bash
# ~/bin/kcode-focus    env: JCODE_FOCUS_SESSION_ID, JCODE_FOCUS_TITLE
tmux select-window -t "$(tmux list-windows -F '#{window_id} #{window_name}' \
  | grep -F "$JCODE_FOCUS_TITLE" | head -1 | cut -d' ' -f1)"
```

If the hook fails to start, kcode falls back to its built-in focus path.

### Which terminal the hook targets

The server is long-lived and captures terminal-identifying env once at startup
(`TMUX`, `ZELLIJ_SESSION_NAME`, `KITTY_WINDOW_ID`, `DISPLAY`, …). A client that
connects later snapshots its own values and sends them to the server, and the
server re-exports them when running a spawn hook - so the hook follows the
terminal you are actually attached to, not the one the server started in.
Multiplexers (tmux, zellij, screen), terminal emulators and the display server
are covered, and the same values are also exposed as `JCODE_CLIENT_<NAME>`
aliases for hooks that need to tell the two apart.

Programmatic wrappers can set `JCODE_SPAWN_HOOK` in the environment of the
`kcode` server they launch, and every headed spawn that server performs routes
through the hook.

## Not implemented yet

Real gaps in the hook surface, tracked in [../wip.md](../wip.md):

- **`turn_start` only ever fires with `SOURCE=chat`.** The config schema
  advertises `chat`/`resume`/`ambient`, but `resume` is never passed and
  `ambient` belongs to a mode this fork removed. Either narrow the contract to
  `chat` or wire the resume path.
- **The `session_start` schema comment is stale.** It says `SOURCE` is
  `create`/`resume`; the code emits `create`, `attach` and `resume`.
- **No way to see whether a hook is wired or firing.** There is no `/hooks`
  command, no listing of configured hooks, and no dry-run. A typo in a command
  string is indistinguishable from a hook that runs and does nothing - which is
  the most likely first failure a user hits.
- **Blocked tool calls are invisible to you.** `pre_tool` stderr goes to the
  model; nothing tells the user "your policy blocked 3 calls this session".
- **Hook failures are log-only.** Observers log and drop.
- **The env prefix is still `JCODE_`**, not `KCODE_` (see `wip.md`).
