<div align="center">

# kcode

A lean, focused fork of the [jcode](https://github.com/1jehuang/jcode) coding agent.

Terminal TUI · multi-provider · agentic tools · swarm coordination

</div>

kcode is jcode with a large amount of surface area removed. Same TUI, same
multi-model support, same tools, minus the parts this fork does not want to
carry. Roughly **241,000 lines across 1,079 files** were cut: mermaid/diagram
rendering, inline images, LaTeX, session replay, agent memory and ambient mode,
Gmail/Google login, dictation, the productivity dashboard, the macOS computer-use
tool, the menubar app, the client installer, the iOS app, the telemetry worker,
and the TypeScript SDK. The exact record is in
[docs/plans/KCODE_CUT_MANIFEST.md](docs/plans/KCODE_CUT_MANIFEST.md).

kcode has its own repository and history and does **not** track upstream jcode
commits. It inherits jcode's provider support and auth flows, which are the main
thing to keep an eye on over time.

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
`~/.local/bin`. Build and install from the packaging recipe:

```bash
makepkg -si                 # from packaging/arch/
aur build -d <repo>         # or, with aurutils
```

> Packaging (`packaging/arch/PKGBUILD`) is not written yet; see
> [Status](#status-and-known-gaps). Until then use the source build below and
> avoid copying the binary into `~/.local/bin`, which pacman cannot manage.

### From source

```bash
cargo build --release -p kcode --bin kcode
./target/release/kcode
```

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
| `permissions` | Ambient-mode permission requests |
| `restart` | Save/restore the set of open windows across a reboot |
| `provider-test-coverage` | Live verification coverage summary |
| `provider-doctor` | Walk end-to-end checkpoints to diagnose a broken provider/model |
| `auth-test` | End-to-end auth test: login, credential probe, refresh, smoke |

Global option: `-p/--provider <PROVIDER>` (see [Providers](#providers)).

### Interactive commands

Slash commands inside the TUI. The registry lives in
`crates/jcode-tui/src/tui/app/state_ui_input_helpers.rs` (`REGISTERED_COMMANDS`,
118 entries); these are grouped by function.

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
| `/poke` | Resume with incomplete todos |
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
| `/overnight` | Supervised overnight coordinator |
| `/improve`, `/refactor`, `/test` | Autonomous improve; refactor loop; layered verification |
| `/initiatives`, `/goals` | Initiatives overview |

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
| `/colors` | List, configure, and score every TUI color |
| `/alignment` | Default text alignment |
| `/thinking-display` | Show/hide model thinking (off/full/current) |
| `/tool-call-details` | Dimmed technical details on tool rows |
| `/show-agentgrep-output` | Full agentgrep output inline |
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
| `/update-sim`, `/onboarding-preview`, `/onboarding-sim` | Simulators for update and onboarding UI |
| `/fast-release`, `/fast-macos-release`, `/remote-release` | Release flows |
| `/triage` | Triage and fix safe GitHub issues |
| `/quit` | Exit |

**Hidden** (registered but not advertised): `/model-status`, `/color`, `/todo`,
`/commit-and-push`, `/cut-release`, `/commit-push-release`, `/thinking`,
`/reasoning`, `/clear-view`, `/split`, `/resume-all`, and the premium-mode
commands `/z`, `/zz`, `/zzz`, `/zstatus`.

### Providers

`kcode provider list` reports 26 canonical providers:

| id | provider | auth |
|---|---|---|
| `claude` | Anthropic/Claude | Claude Pro or Max |
| `openai` | OpenAI | ChatGPT Plus or Pro |
| `openrouter` | OpenRouter | API key, 200+ models |
| `azure` | Azure OpenAI | Entra ID or API key |
| `opencode` | OpenCode Zen | API key |
| `opencode-go` | OpenCode Go | API key |
| `zai` | Z.AI Coding Plan | API key |
| `kimi` | Kimi Code | API key |
| `conifer` | Conifer | API key, cost-routed gateway |
| `groq` | Groq | API key |
| `mistral` | Mistral | API key |
| `perplexity` | Perplexity | API key |
| `togetherai` | Together AI | API key |
| `deepinfra` | Deep Infra | API key |
| `novita` | Novita AI | API key |
| `xai` | xAI | API key |
| `grok-build` | Grok Build | subscription |
| `chutes` | Chutes | API key |
| `cerebras` | Cerebras | API key |
| `alibaba-coding-plan` | Alibaba Cloud Coding Plan | API key |
| `openai-compatible` | OpenAI-compatible | base URL + API key |
| `cursor` | Cursor | browser login or API key |
| `copilot` | GitHub Copilot | GitHub device flow |
| `gemini` | Google Gemini | Code Assist OAuth |
| `antigravity` | Antigravity | Google OAuth |
| `auto` | Auto-detect | best configured provider |

The `--provider` flag additionally accepts gateway/alias values that do not
appear in `provider list` (for example `anthropic-api`, `bedrock`,
`hugging-face`, `moonshot-ai`, `nebius`, `scaleway`, `lmstudio`, `ollama`).
See [Status](#status-and-known-gaps).

### Tools

Tools the agent can call. Implementations live in
`crates/jcode-app-core/src/tool/`.

**Filesystem and shell:** `bash`, `read`, `write`, `edit`, `multiedit`,
`patch`, `apply_patch`, `ls`, `glob`, `grep`, `agentgrep` (grep/find/outline/
smart search), `open`, `bg`

**Web:** `webfetch`, `websearch`

**Agent and planning:** `todo`, `task`, `batch`, `swarm`, `communicate`,
`skill` / `skill_manage`, `jcode_docs`

**Memory and retrieval:** `conversation_search`, `session_search`

**Interface:** `panel`, `side_panel`, `browser` (automation)

**Integration:** `mcp` (Model Context Protocol servers), `debug_socket`

**Media:** `image_generation`

### Configuration

Main config: `~/.kcode/config.toml`. Sections include `[server]`,
`[keybindings]`, `[dictation]`, `[display]`, plus provider, agent, hook,
compaction, terminal, auto-review, and auto-judge configuration
(`crates/jcode-config-types/src/lib.rs`).

`~/.config/kcode/` holds app-owned state: `hotkey_usage.json`,
`model_picker_usage.json`, `keybinding_proficiency.json`, `live-tests/`.

Environment overrides (currently `JCODE_*`; see
[Status](#status-and-known-gaps)):

| var | effect |
|---|---|
| `JCODE_HOME` | relocate the state dir (default `~/.kcode`) |
| `JCODE_RUNTIME_DIR` | relocate sockets/durable state |
| `JCODE_SWARM_MAX_CONCURRENT_AGENTS` | swarm concurrency |
| `JCODE_TERMINAL`, `JCODE_SPAWN_HOOK`, `JCODE_FOCUS_HOOK`, `JCODE_HOOK_*` | terminal launch and hooks |

### On-disk layout

```
~/.kcode/                 state (JCODE_HOME)
├── config.toml           main configuration
├── servers.json          daemon registry
├── logs/                 run logs
├── state/                durable state (survives reboot)
└── cache/

~/.config/kcode/          app-owned UI state

$XDG_RUNTIME_DIR/         sockets and ephemeral state (JCODE_RUNTIME_DIR)
├── kcode.sock            main daemon
└── kcode-debug.sock      debug socket
```

kcode keeps its own home, config, and sockets, so it does not collide with an
installed jcode.

---

## Architecture

552 k lines of Rust across 62 crates.

| crate | LOC | role |
|---|---|---|
| `jcode-tui` | 187 k | terminal UI: rendering, input, overlays, session UX |
| `jcode-app-core` | 111 k | agent loop, tools, server, sessions, swarm |
| `jcode-base` | 98 k | config, providers, message/auth core, MCP |
| `jcode-provider-*-runtime` | ~35 k | per-provider wire protocols |
| `jcode-provider-core` | 7.5 k | shared provider abstraction |
| `jcode-tui-markdown` | 7.2 k | markdown rendering |
| `jcode-plan` | 5.8 k | planning |
| `jcode-protocol` | 5.6 k | client/server protocol |
| `jcode-tui-render`, `jcode-render-core` | 9.5 k | render engine |
| `jcode-config-types` | 2.2 k | configuration schema |
| `jcode-storage`, `jcode-core` | 2.9 k | paths, fs, storage primitives |

Plus focused crates for compaction, command risk, import, logging, fuzzy
matching, permissions, usage overlays, workspace, animations, images, swarm,
sessions, tasks, hooks, and the provider catalog.

---

## Status and known gaps

Findings from a top-to-bottom surface pass. None of these are bugs in the
"crashes" sense; they are places where the fork's surface has not caught up
with its intent.

### Fixed

- **`/memory` was dead surface.** Memory was cut, but the command stayed
  registered and advertised while both handlers (local and remote) only printed
  a usage error. Command, suggestion branch, argument-accepts entry, handlers,
  and help entries removed.
- **The default config template shipped an `[ambient]` section** for a cut
  feature, and its test still asserted a removed `memory_model` key. Both gone.
- **The whole jcode.sh account / subscription / hosted-model surface was
  removed** (commit `59af6e1b`, 64 files). It was upstream's service, not
  something this fork hosts: `kcode account login/status/manage/logout`, the
  `jcode` provider, `/subscription`, `/subscribe`, `/hosted`, `/support`, the
  hosted-model nudge, and the `subscription_api` / `subscription_catalog` /
  `account_login` / `provider/jcode` modules. The login-import summary no
  longer offers a subscription pill. Consequence: no route to Jcode's hosted
  models; `/account` and `/accounts` remain as the multi-account picker for
  Claude/OpenAI.

### Open

- **Environment variables still use `JCODE_`.** `JCODE_HOME`,
  `JCODE_RUNTIME_DIR`, and the hook vars were never renamed, even though the
  state dir is `~/.kcode`. Decision needed: rename to `KCODE_*` (with a
  `JCODE_*` fallback) or document as-is.
- **Config files carry unknown sections.** The schema has 18 top-level
  sections; older configs still hold `[dictation]`, `[ambient]`, `[safety]`,
  `[gateway]`, `[launch_hotkeys]`, and `display.diagram_mode` /
  `latex_rendering` / `pin_images`. The loader ignores unknown keys silently, so
  they are harmless but misleading. `~/.kcode/config.toml` was cleaned in place.
- **Dead command names in the SSH block list**: `/theme`, `/stats`, `/file`,
  `/open`, `/permission`, `/permissions`, `/new-terminal`, `/debug-fixture` are
  blocked over SSH but have no handler anywhere, so they do nothing locally
  either. Remove them (and the SSH assertions that name them) or implement them.
- **CLI advertises 52 `--provider` values but `provider list` shows 27.** The
  extra values are gateways/aliases with no catalog entry.
- **`/help <item>` detail is incomplete.** The `/help` overlay itself is
  complete: curated sections plus a `More commands` section that auto-lists
  every remaining registered command. But `/help <item>` has written detail for
  only 70 of the 118; the rest answer `Unknown command`, and there is no
  compact `/help list`. Planned (deferred): keep `/help` curated, add
  `/help list` listing all commands several per line, and let `/help <item>`
  fall back to the registered one-line description.
- **Packaging does not exist yet** (`packaging/arch/PKGBUILD`); the README
  previously recommended an unmanaged `install -Dm755` into `~/.local/bin`.

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

Inherited from jcode. See [LICENSE](LICENSE).
