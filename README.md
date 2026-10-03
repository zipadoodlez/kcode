<div align="center">

# kcode

A lean, focused fork of the [jcode](https://github.com/1jehuang/jcode) coding agent.

Terminal TUI · multi-provider · agentic tools · swarm coordination

</div>

kcode is jcode with a large amount of surface area removed. Same TUI, same
multi-model support, same tools, minus the parts this fork does not want to
carry. Roughly **241,000 lines across 1,079 files** of diagram rendering,
memory, ambient mode, hosted models and platform clients were cut; see
[what-was-removed.md](docs/what-was-removed.md) for the full record.

kcode has its own repository and history and does **not** track upstream jcode
commits. It inherits jcode's provider support and auth flows, which are the main
thing to keep an eye on over time.

On MIT code, a subtraction fork can only own what it *adds*. What kcode adds is
verifiability and package-manager ownership: reproducible builds, no
self-modification, no telemetry, permission-gated by default, and performance
claims that ship with a runnable script and its raw artifacts in-repo.

---

## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [Surface map](#surface-map)
  - [CLI commands](#cli-commands)
  - [Interactive commands](#interactive-commands)
  - [Providers](#providers)
  - [Tools](#tools)
  - [Configuration](#configuration)
  - [On-disk layout](#on-disk-layout)
- [Architecture](#architecture)
- [Status and known gaps](#status-and-known-gaps)
- [Development](#development)
- [Licence](#licence)

---

## Install

### Arch Linux (preferred)

kcode is meant to be owned by the package manager, not dropped into
`~/.local/bin`. The package is `kcode-git` on the AUR, because kcode has no tags
to build a release from. The recipe lives in the AUR repo, not in this tree, so
clone the package first.

With makepkg, which installs directly:

```bash
git clone https://aur.archlinux.org/kcode-git.git
cd kcode-git
makepkg -si
```

With aurutils, which builds into a local repository and leaves installation to
pacman. Point aurutils at your repository once:

```bash
mkdir -p ~/.config/aurutils
echo 'AUR_REPO=custom' >> ~/.config/aurutils/env
```

then, from the package clone:

```bash
cd kcode-git
aur build                    # build, add to [custom], refresh its database

# Install from a directory without the makepkg workspace: pacman would read a
# local `kcode-git` path there. No -y: [custom] is a local file:// repository.
sudo pacman -S kcode-git
```

`aur build -c` builds inside a systemd-nspawn container instead of the host.
Both paths need network, because the recipe clones the source and cargo fetches
crates, including the `kgrep` git dependency. A cold build takes about 15
minutes.

### From source

```bash
cargo build --release -p kcode --bin kcode
./target/release/kcode
```

kcode does not install, update, or repoint itself. Older installs may still carry a
`~/.kcode/builds` tree from the upstream self-dev machinery; it is inert.

---

## Quick start

```bash
kcode                 # launch the TUI
kcode run 'explain this repo'   # one message, then exit
kcode repl            # plain REPL, no TUI
kcode login           # authenticate a provider
kcode server start    # background daemon (TUI clients attach to it)
kcode server stop
```

Inside the TUI, type `/` for the command palette and `/help` for help.

### Connecting a provider

kcode does not prompt you to log in. It shows a single line telling you to when
it has no provider it could send a request to, and otherwise starts working.

```bash
kcode login                     # pick a provider interactively (OAuth or API key)
kcode login --provider openai   # a specific one
printf '%s' "$THE_API_KEY" | kcode provider add my-endpoint \
    --base-url https://api.example.com/v1 --model <model> --api-key-stdin
```

If you already use another agent CLI, `kcode login` offers to import its logins
rather than making you authenticate again. An API-key provider also works purely
from the environment: set the variable named in `kcode provider list` (for
example `DEEPSEEK_API_KEY`) and it is usable with no further setup.

`kcode auth doctor` reports what is broken when a provider is configured but
rejected, with the recovery steps for each case.

---

## Surface map

Everything the binary exposes, top to bottom. This section doubles as the map
of what the application actually is.

### CLI commands

Top-level `kcode <command>`:

| Command | Purpose |
|---|---|
| `serve` | Start the agent server (background daemon) |
| `acp` | Run as an Agent Client Protocol (ACP) adapter backed by the daemon |
| `server` | Manage the daemon: `start`, `reload`, `stop` |
| `connect` | Connect to a running server |
| `run` | Run a single message and exit (`--json` for machine output) |
| `login` | Log in to a provider via OAuth, API key, or local credentials |
| `logout` | (via `/logout`) log out of a provider |
| `repl` | Simple REPL mode (no TUI) |
| `version` | Build/version info, human or `--json` |
| `usage` | Usage limits for connected providers |
| `debug` | Debug socket CLI against a running server |
| `auth` | Auth status and validation: `import`, `status`, `doctor` |
| `provider` | Provider discovery: `list`, `current`, `add` |
| `session` | Session management: `rename` |
| `model` | Model management: `list` |
| `browser` | Browser automation setup and status |
| `transcript` | Inject externally transcribed text into the active TUI |
| `restart` | Save/restore the set of open windows across a reboot |
| `provider-test-coverage` | Live verification coverage summary |
| `provider-doctor` | Walk end-to-end checkpoints to diagnose a broken provider/model |
| `auth-test` | End-to-end auth test: login, credential probe, refresh, smoke |

Global option: `-p/--provider <PROVIDER>` (see [Providers](#providers)).

### Interactive commands

Slash commands inside the TUI. The registry lives in
`crates/kcode-tui/src/tui/app/state_ui_input_helpers.rs` (`REGISTERED_COMMANDS`,
114 entries); these are grouped by function.

**Help and meta**

| | |
|---|---|
| `/help`, `/?`, `/commands` | Help and keyboard shortcuts |
| `/hotkeys` | Hotkeys with personal usage counts |
| `/version`, `/changelog`, `/info` | Version, recent changes, session/token info |
| `/log` | Mark the current location in the logs |

**Model and provider**

| | |
|---|---|
| `/model`, `/models` | List or switch models |
| `/refresh-model-list` | Refresh provider model catalogs |
| `/agents` | Configure models for agent roles |
| `/subagent`, `/subagent-model` | Launch a subagent; its model policy |
| `/provider-test-coverage` | Live-test evidence for the current provider/model |
| `/usage` | Connected provider usage limits |
| `/effort` | Reasoning effort |
| `/fast` | Toggle fast mode |

**Conversation and context**

| | |
|---|---|
| `/clear`, `/cls` | Clear history; clear view only |
| `/compact` | Compact context |
| `/rewind` | Rewind to a previous message |
| `/transfer` | Compact context into a fresh handoff session |
| `/context` | Full session context snapshot |
| `/cache` | Cache stats / TTL |
| `/fix` | Recover when the model cannot continue |
| `/plan` | Plan-only response as a plan card |

**Sessions**

| | |
|---|---|
| `/resume`, `/sessions`, `/session` | Session picker |
| `/active` | Live sessions (working vs ready) |
| `/catchup`, `/back` | Catch up picker; return to previous |
| `/save`, `/unsave`, `/rename` | Bookmark; unbookmark; rename |
| `/fork` | Fork session into a new window |
| `/workspace` | Niri-style session workspace |
| `/continue`, `/resumeall` | Continue interrupted live sessions |

**Tools, agents, automation**

| | |
|---|---|
| `/todos`, `/todo` | Session todo list |
| `/observe` | Latest tool context in the side panel |
| `/splitview`, `/split-view` | Mirror chat in the side panel |
| `/btw` | Ask a side question in the side panel |
| `/skills` | Loaded skills and recommendations |
| `/swarm`, `/swarm-prompt` | Swarm feature; routing prompt |
| `/autoreview`, `/autojudge` | Automatic end-of-turn review/judging |
| `/review`, `/judge` | One-shot review/judge session |
| `/improve`, `/refactor`, `/test` | Autonomous improve; refactor loop; layered verification |

**Workflow and git**

| | |
|---|---|
| `/git` | Git status for the session working directory |
| `/diff` | Diff display mode (off/inline/full/pinned/file) |
| `/commit`, `/commit-push` | Logical commits; commits then push |
| `/ssh` | Connect to a remote machine over system SSH |
| `/transcript` | Open the session transcript file |

**Display and appearance**

| | |
|---|---|
| `/colors` | List and configure palette slots and color roles |
| `/alignment` | Default text alignment |
| `/thinking-display` | Show/hide model thinking (off/full/current) |
| `/tool-call-details` | Dimmed technical details on tool rows |
| `/show-kgrep-output` | Full kgrep output inline |
| `/compact-notifications` | Single-line swarm/file-activity notifications |
| `/terminal-setup` | Fix Shift+Enter newlines |
| `/debug-visual`, `/screenshot-mode`, `/screenshot`, `/record` | Visual debug and capture |

**Account and auth**

| | |
|---|---|
| `/auth`, `/login`, `/logout` | Auth status; log in; log out |
| `/account`, `/accounts` | Combined account picker (Claude/OpenAI multi-account) |

**Lifecycle**

| | |
|---|---|
| `/reload`, `/restart`, `/rebuild`, `/update` | Restart / background rebuild / update |
| `/client-reload`, `/server-reload` | Force reload client or server binary |
| `/selfdev` | New self-dev session |
| `/update-sim` | Simulator for update UI |
| `/fast-release`, `/fast-macos-release`, `/remote-release` | Release flows |
| `/triage` | Triage and fix safe GitHub issues |
| `/quit` | Exit |

**Hidden** (registered but not advertised): `/model-status`, `/color`, `/todo`,
`/commit-and-push`, `/cut-release`, `/commit-push-release`, `/thinking`,
`/reasoning`, `/clear-view`, `/split`, `/resume-all`, and the premium-mode
commands `/z`, `/zz`, `/zzz`, `/zstatus`.

### Providers

`kcode provider list` prints every id `-p/--provider` accepts, each with its
auth kind; `--json` prints the same list for scripting. The flag also accepts
aliases for the same backends. Run the command rather than reading a copy here:
the list is derived from the provider registry, so it cannot go stale.

`kcode provider current` shows what was requested and what actually resolved.
See [docs/user/providers.md](docs/user/providers.md) for login flows, credential
locations, and per-provider notes.

### Tools

Tools the agent can call. Implementations live in
`crates/kcode-app-core/src/tool/`.

**Filesystem and shell:** `bash`, `read`, `write`, `edit`, `multiedit`,
`patch`, `apply_patch`, `ls`, `kgrep` (grep/find/outline/
trace modes), `open`, `bg`

**Web:** `webfetch`, `websearch`

**Agent and planning:** `todo`, `task`, `batch`, `swarm`, `communicate`,
`skill` / `skill_manage`, `kcode_docs`

**Memory and retrieval:** `conversation_search`, `session_search`

**Interface:** `panel`, `side_panel`, `browser` (automation)

**Integration:** `mcp` (Model Context Protocol servers), `debug_socket`

**Media:** `image_generation`

### Configuration

Main config: `~/.kcode/config.toml`. Sections include `[server]`,
`[keybindings]`, `[display]`, plus provider, agent, hook,
compaction, terminal, auto-review, and auto-judge configuration
(`crates/kcode-config-types/src/lib.rs`).

`~/.config/kcode/` holds app-owned state: `hotkey_usage.json`,
`model_picker_usage.json`, `keybinding_proficiency.json`, `live-tests/`.

Environment overrides (currently `KCODE_*`; see
[Status](#status-and-known-gaps)):

| var | effect |
|---|---|
| `KCODE_HOME` | relocate the state dir (default `~/.kcode`) |
| `KCODE_RUNTIME_DIR` | relocate sockets/durable state |
| `KCODE_SWARM_MAX_CONCURRENT_AGENTS` | swarm concurrency |
| `KCODE_TERMINAL`, `KCODE_SPAWN_HOOK`, `KCODE_FOCUS_HOOK`, `KCODE_HOOK_*` | terminal launch and hooks |

### On-disk layout

```
~/.kcode/                 state (KCODE_HOME)
├── config.toml           main configuration
├── servers.json          daemon registry
├── logs/                 run logs
├── state/                durable state (survives reboot)
└── cache/

~/.config/kcode/          app-owned UI state

$XDG_RUNTIME_DIR/         sockets and ephemeral state (KCODE_RUNTIME_DIR)
├── kcode.sock            main daemon
└── kcode-debug.sock      debug socket
```

kcode keeps its own home, config, and sockets, so it does not collide with an
installed kcode.

---

## Architecture

552 k lines of Rust across 62 crates.

| crate | LOC | role |
|---|---|---|
| `kcode-tui` | 187 k | terminal UI: rendering, input, overlays, session UX |
| `kcode-app-core` | 111 k | agent loop, tools, server, sessions, swarm |
| `kcode-base` | 98 k | config, providers, message/auth core, MCP |
| `kcode-provider-*-runtime` | ~35 k | per-provider wire protocols |
| `kcode-provider-core` | 7.5 k | shared provider abstraction |
| `kcode-tui-markdown` | 7.2 k | markdown rendering |
| `kcode-plan` | 5.8 k | planning |
| `kcode-protocol` | 5.6 k | client/server protocol |
| `kcode-tui-render`, `kcode-render-core` | 9.5 k | render engine |
| `kcode-config-types` | 2.2 k | configuration schema |
| `kcode-storage`, `kcode-core` | 2.9 k | paths, fs, storage primitives |

Plus focused crates for compaction, command risk, import, logging, fuzzy
matching, usage overlays, workspace, terminal images, swarm, sessions, tasks,
hooks, and the provider catalog.

---

## Status and known gaps

Work in progress has one entry point: **[docs/plans/](docs/plans/)**, one lane per
subject, each holding its own task checklist with the design beside the item. What a
cut cost is recorded in **[docs/what-was-removed.md](docs/what-was-removed.md)**. This
README does not duplicate any of them.

---

## Development

```bash
cargo build --release -p kcode --bin kcode   # ~15 min cold, seconds warm
cargo test                                    # workspace tests
```

Build cache: the dev profile has `incremental = true`, and
`target/debug/incremental` grows without bound. If disk matters, delete it
periodically; `target/debug/deps` is the part that actually keeps rebuilds fast.

Use `cargo` for the edit-run loop and a PKGBUILD for installing, not the other
way around.

---

## Licence

Inherited from kcode. See [LICENSE](LICENSE).
