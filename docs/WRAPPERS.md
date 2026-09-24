# kcode wrapper / scripting guide

This document describes the non-interactive CLI surface intended for wrappers, scripts, and other tools that invoke `kcode`.

## Recommended flags

Use these flags by default in wrappers:

```bash
kcode --quiet --no-update --no-selfdev ...
```

- `--quiet` suppresses non-error CLI/status chatter
- `--no-update` avoids update-check noise/work
- `--no-selfdev` avoids repository auto-detection changing runtime behavior

## Discover available models

List model names that can be passed to `-m/--model`:

```bash
kcode --quiet model list
kcode --quiet model list --json
kcode --quiet --provider openai model list --json
```

## Discover providers and current selection

List provider IDs you can pass to `-p/--provider`:

```bash
kcode --quiet provider list
kcode --quiet provider list --json
```

Inspect the currently requested and resolved provider/model selection:

```bash
kcode --quiet provider current
kcode --quiet --provider openai --model gpt-5.4 provider current --json
```

Verbose human summary:

```bash
kcode --quiet model list --verbose
```

## Run one prompt and return JSON

```bash
kcode --quiet run --json "Reply with exactly OK"
```

## Stream one prompt as NDJSON

```bash
kcode --quiet run --ndjson "Reply with exactly OK"
```

Typical event types:

- `start`
- `connection_phase`
- `connection_type`
- `text_delta`
- `text_replace`
- `tool_start`
- `tool_input`
- `tool_exec`
- `tool_done`
- `tokens`
- `done`
- `error`

The final `done` event includes the assembled text and usage summary.

Example shape:

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

## Inspect authentication state

```bash
kcode --quiet auth status
kcode --quiet auth status --json
```

JSON output includes:

- `any_available`
- `providers[]`
  - `id`
  - `display_name`
  - `status`
  - `method`
  - `auth_kind`
  - `recommended`

## Inspect build/version details

```bash
kcode --quiet version
kcode --quiet version --json
```

JSON output includes:

- `version`
- `git_hash`
- `git_tag`
- `build_time`
- `git_date`
- `release_build`

## Notes

- JSON commands are designed so the intended machine-readable result is printed to `stdout`
- With `--quiet`, wrapper-oriented commands should keep `stderr` empty unless there is a real warning/error
- `kcode model list` and `kcode run --json` do not require the TUI
- `kcode model list` does not require an already-running shared server
