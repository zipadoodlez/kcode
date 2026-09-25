# Architecture

kcode is a single-server, multi-client app. One daemon owns every session; TUI
clients connect over a Unix socket and reconnect transparently after a drop or a
server reload.

## Server and clients

- The daemon owns sessions, providers, and a shared MCP pool.
- Clients connect to it and attach to a session.
- Killing a client never affects the server or other clients.
- The server has a random adjective/verb name and each session an animal noun,
  shown as e.g. "blazing fox". The server name persists across reloads in the
  registry; stale entries are cleaned up.

Socket and state locations:

| path | what |
|---|---|
| `<runtime_dir>/jcode/jcode.sock` | main client socket |
| `<runtime_dir>/jcode/` | also holds the debug socket |
| `~/.kcode/servers.json` | server registry |

`runtime_dir` is `JCODE_RUNTIME_DIR` if set, else `XDG_RUNTIME_DIR` (Linux,
normally `/run/user/$UID`), else `TMPDIR` on macOS, else a private `kcode-<user>`
directory under the system temp dir.

## Lifecycle

**Startup.** Running `kcode` connects to the server if one exists, otherwise
spawns `kcode serve` detached with `setsid()`, waits for the socket, and connects.

**Idle shutdown.** The daemon exits after 300 seconds with no clients connected
and no live headless swarm workers (`IDLE_TIMEOUT_SECS`). The monitor checks
every 10 seconds, so shutdown can land slightly later. Debug-control servers
disable this; temporary servers use a separate policy.

**Reload.** `/reload` makes the server `exec()` into the new binary on the same
socket. All clients disconnect and auto-reconnect, and the initiating client
re-execs itself if its binary is older.

**Client reconnect.** On a dropped connection the client shows "Connection lost -
reconnecting..." and retries with exponential backoff (1s to 30s), then resumes
the same session, whose state persists on disk.

### Ownership markers

`~/.kcode/active_pids/<session_id>` holds the PID that owns a session. In server
mode that is the daemon PID, so sessions share a PID. Despite the name, this
means *process ownership*, not an open window or a connected client: a
daemon-held session stays registered after its client disconnects. Markers can be
left behind by an exited process, so a marker alone does not prove liveness;
`session_presence()` filters dead owners while `active_session_ids()` does not.
Separate `streaming_pids` markers track model-response generation.

### Remote working directory

A client sends its cwd on subscribe. Socket-forwarding wrappers can separate the
two paths:

```sh
kcode --socket /tmp/jcode.sock -C /local/checkout --remote-working-dir /remote/checkout
```

`-C` must exist locally; `--remote-working-dir` must be an absolute path that
exists on the server. (Native SSH attach is separate; see
[../user/ssh.md](../user/ssh.md).)

## Self-dev mode

Running `kcode` inside the kcode repository auto-detects the repo and marks that
session as canary/self-dev via subscribe metadata, enabling the self-dev prompt
and tooling for that session only. `/reload` still hot-reloads the shared server.

## Session, surface, client

- A **session** is a server-owned runtime: conversation history, provider/model
  state, tool state, persistence, and background tasks. It is not a window.
- A **surface** is a client's view of a session.
- A **client** is a process hosting one or more surfaces.

There is one active interactive surface per session. Client 1 attaching to
sessions A/B/C is the workspace mode: a Niri-style camera workspace that shows
one full-size session at a time, with a **workspace map** widget rendering rows of
sessions as rectangles (idle, focused, running, completed, waiting, error,
detached) and movement between sessions/rows. Workspace navigation keys currently
register in remote (SSH) mode. External window managers stay first-class:
`kcode --resume <id>` opens an independent single-surface client.

## Panels

The model-facing `panel` tool opens and manages desktop panels within a session.
Each spawn creates a fresh panel; panels persist across reconnects, and closing a
linked panel never deletes its source.

Actions: `spawn` (default), `update`, `focus`, `close`, `list`. Spawn/update take
exactly one of `content` (Markdown) or `file_path` (Markdown or PDF). Supported
extensions are `.md`, `.markdown`, `.mdown`, `.mkd`, `.mkdn`, `.pdf`
(case-insensitive); relative paths resolve from the tool working directory. Files
are linked, so reload reads their current bytes; PDF validation only requires a
`%PDF-` signature.

PDFs are capped at **20 MiB per file** and **32 MiB per session**
(`MAX_PDF_BYTES`, `MAX_SESSION_PDF_BYTES`), and `content` stays human-readable
Markdown, never base64. Native clients opt in to PDF payloads with
`supports_pdf_panels: true`; others get Markdown projections. The legacy
`side_panel` tool remains registered for compatibility (`status`, `write`,
`append`, `load`, `focus`, `delete`), with write/append Markdown-only.

## Terminal routing

Headed session launches route through the shared terminal launcher, which covers
visible swarm spawns, resume-in-new-terminal, self-dev launches, and restart
restores. A configured `[terminal].spawn_hook` takes precedence (see
[../user/hooks.md](../user/hooks.md)).

For Herdr, a launch requested from a client with `HERDR_ENV=1` and
`HERDR_PANE_ID` splits the calling pane to the right, focuses it, and starts the
resumed session there; `HERDR_BIN_PATH` is honored when present. The launcher
forwards `HERDR_ENV`, `HERDR_SOCKET_PATH`, `HERDR_PANE_ID`, `HERDR_TAB_ID`,
`HERDR_WORKSPACE_ID`, `HERDR_BIN_PATH`, `HERDR_SESSION`, and `HERDR_AGENT` from
the requesting client to the server's spawn and focus paths.
