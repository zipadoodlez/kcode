# CLI

kcode is primarily a TUI, but its non-interactive surface is meant for wrappers
and scripts. Interactive use is in [tui.md](tui.md).

## Flags for wrappers

```sh
kcode --quiet --no-update --no-selfdev ...
```

- `--quiet` - suppress non-error CLI/status output, so `stderr` stays empty
  unless there is a real warning or error.
- `--no-update` (and `--auto-update`) - accepted for compatibility and
  **ignored**: updates come from the package manager now.
- `--no-selfdev` - disable repository auto-detection, which otherwise changes
  runtime behavior when you happen to be inside a kcode checkout.

## Commands

| command | output |
|---|---|
| `kcode model list [--json] [--verbose]` | models you can pass to `-m/--model` |
| `kcode provider list [--json]` | provider ids you can pass to `-p/--provider` |
| `kcode provider current [--json]` | requested vs resolved provider/model |
| `kcode run [--json\|--ndjson] "<prompt>"` | one message, then exit |
| `kcode auth status [--json]` | credential state per provider |
| `kcode version [--json]` | version and build details |

Machine-readable results go to stdout. `model list` and `run --json` do not need
the TUI, and `model list` does not need a running shared server.

### `run`

```sh
kcode --quiet run --json "Reply with exactly OK"
kcode --quiet run --ndjson "Reply with exactly OK"
```

`--json` prints one result object; `--ndjson` streams one JSON event per line.
Event types: `start`, `connection_phase`, `connection_type`, `text_delta`,
`text_replace`, `tool_start`, `tool_input`, `tool_exec`, `tool_done`, `tokens`,
`done`, `error`. The final `done` event carries the assembled text and usage:

```json
{
  "session_id": "session_...",
  "provider": "OpenAI",
  "model": "gpt-5.4",
  "text": "OK",
  "usage": {
    "input_tokens": 123,
    "output_tokens": 7,
    "cache_read_input_tokens": 0,
    "cache_creation_input_tokens": null
  }
}
```

### `auth status --json`

```json
{ "any_available": true, "providers": [ { "id": "...", "display_name": "...",
  "status": "...", "method": "...", "auth_kind": "...", "recommended": true } ] }
```

See [auth.md](auth.md) for what the statuses mean and where credentials live.

### `version --json`

Fields: `version`, `git_hash`, `git_tag`, `build_time`, `git_date`,
`release_build` (a clean release build is `true`, a dev build `false`).
