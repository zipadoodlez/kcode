<div align="center">

# jcode

[![CI](https://github.com/1jehuang/jcode/actions/workflows/ci.yml/badge.svg)](https://github.com/1jehuang/jcode/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/Built%20with-Rust-orange.svg)](https://www.rust-lang.org/)

A blazing-fast, fully autonomous AI coding agent with a gorgeous TUI,
multi-model support, swarm coordination, persistent memory, and 30+ built-in tools -
all running natively in your terminal.

<br>

<img src="https://github.com/1jehuang/jcode/releases/download/v0.3.1/jcode_demo_jaguar.avif" alt="jcode demo" width="800">

<br>

[Features](#features) · [Install](#installation) · [Quick Start](#quick-start) · [Further Reading](#further-reading)

</div>

---

<div align="center">

## Installation

### Setup

If you want another agent to set up jcode for you, give it this prompt:

```text
Set up jcode on this machine for me.

1. Detect the operating system and choose the best install method:
   - macOS: prefer Homebrew if available, otherwise use the install script
   - Linux: use the install script
   - Windows: use the PowerShell install script
2. Verify that `jcode` is on my `PATH`.
3. Launch `jcode` once in a new terminal window/session to confirm it starts successfully.
4. Before attempting any interactive login flow, assess which providers are already available non-interactively and prefer those first. Check existing local credentials, config files, CLI sessions, and environment variables such as:
   - Claude: `~/.jcode/auth.json`, `~/.claude/.credentials.json`, `~/.local/share/opencode/auth.json`, `ANTHROPIC_API_KEY`
   - OpenAI: `~/.jcode/openai-auth.json`, `~/.codex/auth.json`, `OPENAI_API_KEY`
   - Gemini: `~/.jcode/gemini_oauth.json`, `~/.gemini/oauth_creds.json`
   - GitHub Copilot: existing auth under `~/.config/github-copilot/`
   - Azure OpenAI: `~/.config/jcode/azure-openai.env`, `AZURE_OPENAI_*`, or an existing `az login`
   - OpenRouter: `OPENROUTER_API_KEY`
   - Alibaba Cloud Coding Plan: existing jcode config/env if present
5. Prefer whichever provider is already configured and verify it with `jcode auth-test --all-configured` or a provider-specific auth test when appropriate.
6. Only if no usable provider is already configured, guide me through the minimal manual step needed:
   - Claude: `jcode login --provider claude`
   - GitHub Copilot: `jcode login --provider copilot`
   - OpenAI: `jcode login --provider openai`
   - Gemini: `jcode login --provider gemini`
   - Azure OpenAI: `jcode login --provider azure`
   - Alibaba Cloud Coding Plan: `jcode login --provider alibaba-coding-plan`
   - OpenRouter: help me set `OPENROUTER_API_KEY`
   - Anthropic direct API: help me set `ANTHROPIC_API_KEY`
7. After setup, run a simple smoke test with `jcode run "say hello"` and confirm it works.
8. Explain any manual step that still needs me, especially browser OAuth, device login, or API key entry.
```

This is intended to be a copy-paste bootstrap prompt for jcode itself or any other coding agent.

### Quick Install

```bash
# macOS & Linux
curl -fsSL https://raw.githubusercontent.com/1jehuang/jcode/master/scripts/install.sh | bash
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/1jehuang/jcode/master/scripts/install.ps1 | iex
```

### macOS via Homebrew

```bash
brew tap 1jehuang/jcode
brew install jcode
```

### From Source (all platforms)

```bash
git clone https://github.com/1jehuang/jcode.git
cd jcode
cargo build --release
```

For local self-dev / refactor work on Linux x86_64, prefer:

```bash
scripts/dev_cargo.sh build --release -p jcode --bin jcode
scripts/dev_cargo.sh --print-setup
```

That wrapper automatically uses `sccache` when available, prefers a fast
working local linker setup (`clang + lld`) instead of assuming every machine's
`mold` configuration is valid, and can print the active linker/cache setup via
`--print-setup` so slow-path builds are easier to diagnose.

Then symlink to your PATH:

```bash
scripts/install_release.sh
```

### Prerequisites

You need at least one of:

| Provider | Setup |
|---|---|
| **Claude** (recommended) | Run `/login claude` inside jcode (opens browser for OAuth) |
| **GitHub Copilot** | Run `/login copilot` inside jcode (GitHub device flow) |
| **OpenAI** | Run `/login openai` inside jcode (opens browser for OAuth) |
| **Google Gemini** | Run `/login gemini` inside jcode (native Google OAuth for Code Assist) |
| **Azure OpenAI** | Run `jcode login --provider azure` (Microsoft Entra ID or API key) |
| **Alibaba Cloud Coding Plan** | Run `jcode login --provider alibaba-coding-plan` (Alibaba Cloud Bailian API key) |
| **OpenRouter** | Set `OPENROUTER_API_KEY=sk-or-v1-...` |
| **Direct API Key** | Set `ANTHROPIC_API_KEY=sk-ant-...` |

### Platform Support

| Platform | Status |
|---|---|
| **Linux** x86_64 / aarch64 | Fully supported |
| **macOS** Apple Silicon & Intel | Supported |
| **Windows** x86_64 | Supported (native + WSL2) |

</div>

---

## Features

<div align="center">

| Feature | Jump | Description |
|---|---|---|
| **Resource / Performance** | [Open section](#performance--resource-efficiency) | Startup time, FPS benchmarks, RAM footprint, and parallel usage demos |
| **Memory** | [Open section](#memory) | Persistent cross-session memory for preferences, facts, entities, and corrections |
| **Side Panel and Generated UI** | [Open section](#side-panel-and-generated-ui) | Rich in-terminal UI panels, markdown rendering, and generated visual output |
| **Swarm** | [Open section](#swarm) | Multiple agents coordinating in the same repo with communication and task sharing |
| **OAuth and Providers** | [Open section](#oauth-and-providers) | Use Claude Max, ChatGPT Pro, GitHub Copilot, Gemini, and more without juggling raw API keys |
| **Self-Dev** | [Open section](#self-dev) | Built-in self-development workflow with release builds, reload, and debug tooling |
| **iOS Application for Native OpenClaw** | [Open section](#ios-application-for-native-openclaw) | Coming soon: a native iOS experience for OpenClaw ambient workflows |

</div>

---

<div align="center">

## Performance & Resource Efficiency

*A single native binary. No Node.js. No Electron. No Python. Just Rust.*

</div>

jcode is built to stay fast and light even when you launch many sessions in sequence,
keep multiple clients attached, and run agents in parallel.

<!-- Add performance demo thumbnail/video link here: spawning many jcode instances and using them in parallel. -->

### Headline numbers

- **Startup time:** **Instant**
- **Rendering:** **0.67 ms** average frame time, or roughly **1,400–1,500 FPS**
- **Idle client RAM:** **~28 MB**
- **Base server RAM:** **~40 MB**
- **Active session RAM:** **~50–65 MB**
- **Idle CPU:** **~0.3%**

### Benchmarks

<div align="center">

| Metric | jcode | Typical AI IDE / Agent |
|---|---|---|
| **Startup time** | **Instant** | 3–10 seconds |
| **Frame render time** | **0.67 ms** | 16 ms (60 FPS, if lucky) |
| **Rendering throughput** | **~1,400–1,500 FPS** | ~60 FPS |
| **Idle client memory** | **~28 MB** | 300–800 MB |
| **Server memory** | **~40 MB** (base) | N/A (monolithic) |
| **Active session memory** | **~50–65 MB** | 500 MB+ |
| **CPU at idle** | **~0.3%** | 2–5% |
| **Runtime dependencies** | **None** | Node.js, Python, Electron, … |
| **Binary** | **Single 66 MB executable** | Hundreds of MB + package managers |

</div>

---

## Memory

Persistent memory lets jcode remember facts, preferences, entities, and corrections across sessions.

<!-- Add memory demo thumbnail/video and fuller writeup here. -->

---

## Side Panel and Generated UI

The side panel can render linked markdown, diagrams, and generated visual output directly inside the terminal workflow.

<!-- Add side panel / generated UI demo thumbnail/video and fuller writeup here. -->

---

## Swarm

Swarm mode lets multiple agents coordinate inside the same repo with messaging, task sharing, and conflict-aware collaboration.

<!-- Add swarm demo thumbnail/video and fuller writeup here. -->

---

## OAuth and Providers

jcode works with subscription-backed OAuth flows and multiple providers, so you can use the models you already pay for.

<!-- Add OAuth / provider demo thumbnail/video and fuller writeup here. -->

---

## Self-Dev

jcode ships with a built-in self-development workflow for release builds, reloads, and debug-driven iteration.

<!-- Add self-dev demo thumbnail/video and fuller writeup here. -->

---

## iOS Application for Native OpenClaw

A native iOS implementation of OpenClaw is coming soon.

<!-- Add iOS / native OpenClaw preview and fuller writeup here. -->

---

<div align="center">

## Quick Start

</div>

```bash
# Launch the TUI
jcode

# Run a single command non-interactively
jcode run "say hello"

# Resume a previous session by memorable name
jcode --resume fox

# Run as a persistent background server, then attach more clients
jcode serve
jcode connect

# Send voice input from your configured STT command
jcode dictate
```

jcode supports interactive TUI use, non-interactive runs, persistent server/client workflows,
and hotkey-friendly dictation without requiring a bundled speech-to-text stack.

---

## Further Reading

- [Ambient Mode / OpenClaw](docs/AMBIENT_MODE.md)
- [Memory Architecture](docs/MEMORY_ARCHITECTURE.md)
- [Swarm Architecture](docs/SWARM_ARCHITECTURE.md)
- [Server Architecture](docs/SERVER_ARCHITECTURE.md)
- [iOS Client Notes](docs/IOS_CLIENT.md)
- [Safety System](docs/SAFETY_SYSTEM.md)
- [Windows Notes](docs/WINDOWS.md)
- [Wrappers and Shell Integration](docs/WRAPPERS.md)
- [Refactoring Notes](docs/REFACTORING.md)
