# SSH attach

Native SSH runs the TUI on your machine and leaves the workspace, tools, model
credentials, and agent execution on the SSH host. It is remote **attach**, not
workspace migration or live-process handoff.

## Usage

Install a compatible kcode binary on both hosts, and set up ordinary OpenSSH key
auth with a verified host key first: attach is non-interactive at the SSH layer
and refuses unknown host keys or missing auth.

```sh
kcode --ssh dev --remote-working-dir /srv/kcode
kcode --ssh dev --resume session_remote_id
```

`dev` is an SSH config alias or `user@hostname`. `--ssh-binary /path/to/kcode`
selects the remote executable (default `jcode`). With no `--remote-working-dir`,
the remote daemon's working directory is used, never the local one. `--resume`
takes an explicit remote session ID resolved on the remote server; local session
lookup is never used. `--ssh` conflicts with `--socket`, and provider/tool
startup flags, `--onboarding-sim`, and `--update-sim` are rejected before
connecting.

## Remote login

Authenticate the remote host from the local TUI with `/login`. It uses the same
inline picker as local mode - arrows move, typing filters, Enter selects, Esc
cancels - plus **Import local OpenAI login** and **Import local Claude login**.
It shows the same provider catalog, labels, ordering, and method labels as local
`/login`. The six bridge-supported OAuth routes are OpenAI, Claude, Gemini,
Antigravity, Google, and Copilot. Other rows explain how to set the method up on
the remote host; they never start laptop-local auth or claim the bridge supports
them.

The destination notice names the SSH host. Provider status is fetched from that
host with `kcode auth status --json`, never from local credential stores; an
unknown or unavailable status shows as unknown, not signed out. Only provider
state and fixed method labels are shown, never remote account labels, credential
paths, or raw remote errors.

Browser approval happens on your machine, while pending login records, token
exchange, and saved credentials stay on the host. Treat authorization URLs as
sensitive. Paste the returned callback URL or code into the pending prompt, not
into chat; OpenAI requires the full callback URL even if its localhost page is
unreachable. `/cancel` cancels the pending login, and closing the UI terminates
its owned auth subprocess. Each attempt has its own flow ID, so two clients
cannot replace each other's pending state. Callback input travels over SSH
stdin, never command arguments or chat. Cancellation is not logout.

Google additionally needs its OAuth client configured on the host. Other
credential routes still require `kcode login` on the host. Attach never
auto-copies provider credentials, and never forwards AWS credentials, SSH
agents, or repository contents.

On first attach, an idle client checks the host's login status. If every expected
provider is explicitly unconfigured, it asks **Import a local login first?** with
Yes/No. Yes opens the import picker, No the normal login picker; neither reads or
copies credentials. Missing, failed, or expired status does not count as an empty
host. The offer is shown at most once per launch, and never replaces drafts,
active turns, or an explicit login.

## One-time import of a local login

To skip another browser login on a trusted host, open `/login` in the remote TUI
and choose an **Import local … login** row, or run `/login --import-local openai`
or `/login --import-local claude`. `--import-local` with no provider opens a
picker containing just those two choices. Both routes lead to the same
confirmation, which reads no credentials until approved.

Read the destination-host warning and choose **Yes**. Arrow keys select, or type
`yes`/`no`; **No is the default**, so Enter alone never approves a copy. Esc or
`/cancel` at the prompt copies nothing. On approval, the active account is read
from the local kcode OAuth store and sent over the pinned-host SSH connection
through stdin, never command arguments or chat; the remote CLI stores it
privately and refreshes its provider catalog.

Boundaries:

- It is an explicit one-time copy, not startup sync. The host receives usable
  credentials including refresh credentials; only do it for a host you trust
  with that account.
- OpenAI and Claude **kcode-managed OAuth accounts only**. Other accounts,
  external-tool stores, keychains, environment/API keys, AWS credentials, and
  general config are not imported. Missing, malformed, or expired sources are
  refused; use remote `/login` instead.
- Any existing destination store for that provider is refused, even empty or
  malformed, and Claude's shared `auth.json` is refused when it holds only other
  providers. There is no `--overwrite` switch.
- New files are mode 0600 in a 0700 data directory, published atomically with
  no-replace. Transport is bounded to 64 KiB and never logged. Unix-only.
- Acknowledged import means **credentials were stored**, not that the provider
  accepted them. Refresh-token rotation can invalidate either machine's copy;
  independent remote `/login` avoids sharing refresh state.
- Once transfer starts, cancellation or disconnection cannot promise rollback.
  Check the remote login state before retrying. There is no automatic retry or
  sync.

The receiver `kcode auth import --provider openai --stdin --json` is for the
native client; there is deliberately no CLI command that prints credentials.

## Protocol and compatibility

The client makes a private local Unix socket adapter. Each connection uses an
owned `ssh -T` child to run `kcode server stdio` remotely, which connects to or
starts the native daemon, checks its native SSH capability, emits a bounded
versioned handshake, and carries native JSON frames over stdio.

These frames are not wire-compatible with the TUI socket protocol. The client
checks the actual daemon's native SSH capability, not just the client's version;
an old shared daemon is refused, not silently reloaded or killed.

To test alongside an existing daemon without disturbing it, start a matching
daemon under a separate `JCODE_RUNTIME_DIR` and point
`--ssh-server-socket /remote/runtime/jcode.sock` at it. A socket override alone
does not isolate the daemon lock; a remote wrapper can export both
`JCODE_RUNTIME_DIR` and `JCODE_SOCKET` before exec'ing the new binary.

## Disconnect behavior

SSH clients opt into `Subscribe.continue_on_disconnect`. If a turn is active when
that client disappears, the server retains the turn's supervisor; a new client
can attach to the same session, request full remote history, see new events, and
cancel the turn. Ordinary local clients keep their existing semantics.

Limits:

- No event-cursor replay. Reconnect refreshes remote history instead of
  replaying missed deltas or trusting local transcripts.
- Connection-owned stdin prompts do not migrate across disconnect; pending
  responses fail closed rather than being silently approved.
- Idle detached sessions are not kept alive indefinitely. Completed history
  remains available through normal persistence.
- Stopping the VM or restarting the daemon does not preserve in-flight
  processes. Disk preserves saved files and history, not RAM or running commands.
- A VM SSH-idle shutdown policy still applies; a detached turn cannot override
  it.

Closing the client cleans up its SSH bridges and private socket, not the remote
shared daemon. Keepalives detect a dead connection. Unix clients only.

## Host boundary

The TUI identifies the remote host and does not load, save, or mark remote
sessions in the local session store. Remote sessions do not launch local provider
onboarding. Local-only account, config, file-opener, new-terminal, and reload
actions are guarded rather than interpreting remote paths locally. Provider/tool
startup overrides and `self-dev --build` are rejected in SSH mode; use supported
remote commands or an explicit shell on the host.

## Verification

Three opt-in scripts exercise the real path; each skips without its env vars and
network access.

| script | needs | covers |
|---|---|---|
| `tests/test_native_ssh_cli.py` | `JCODE_NATIVE_SSH_BINARY`, `JCODE_NATIVE_SSH_HOST`, `JCODE_NATIVE_SSH_REMOTE_BINARY`, `JCODE_NATIVE_SSH_CWD` | capability handshake, remote persistence and reattach, refusal of bad cwd/flags, remote history, socket and child cleanup |
| `tests/test_native_ssh_login.py` | `JCODE_NATIVE_SSH_LOGIN=1` plus `JCODE_NATIVE_SSH_BINARY`, `JCODE_NATIVE_SSH_HOST`, `JCODE_NATIVE_SSH_CWD`, `JCODE_NATIVE_SSH_LOGIN_REMOTE_EXECUTABLE` | remote `/login` OAuth initiation and scoped cancellation |
| `tests/test_native_ssh_import.py` | `JCODE_NATIVE_SSH_IMPORT=1` plus `JCODE_NATIVE_SSH_BINARY`, `JCODE_NATIVE_SSH_HOST`, `JCODE_NATIVE_SSH_CWD`, `JCODE_NATIVE_SSH_IMPORT_REMOTE_EXECUTABLE` | explicit import consent, transfer, no-overwrite, secret handling, cleanup |

The `*_REMOTE_EXECUTABLE` values must be absolute paths to the real remote ELF,
not a wrapper. Each script accepts `--self-test` for offline harness safety
checks.

Targeted Rust suites:

```sh
cargo test --lib cli::ssh
cargo test -p jcode-protocol
cargo test -p jcode-tui --lib ssh_remote -- --test-threads=1
cargo test -p jcode-app-core --lib client_disconnect_cleanup -- --test-threads=1
cargo test -p jcode-app-core --lib client_lifecycle -- --test-threads=1
cargo test --test e2e disconnect:: -- --test-threads=1
```

These cover protocol, routing, transport, and controlled-provider lifecycle. They
do not establish real provider approval, external token exchange, or live
inference; those require your provider and are not implied by the synthetic
acceptance.
