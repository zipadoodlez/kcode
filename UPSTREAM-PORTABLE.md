# Portable jcode changes since v0.85.0

Filtered companion to `UPSTREAM-SINCE-0.85.md`. Every item here still has a home
in kcode: the code path it touches exists in this tree. Changes whose subsystem
this fork deleted (voice/dictation, memory/ambient, mermaid/diagram/image/replay,
jcode.sh account/subscription/cloud, desktop applets, SDK, remote compilation,
telemetry worker, macOS menubar/computer-use) are omitted entirely.

Provenance and the full unfiltered record (including what was dropped and why)
live in `UPSTREAM-SINCE-0.85.md`. Mirror:
`~/.kcode/scratch/jcode-upstream.git`. Range: `v0.85.0 .. master@5f1c091cf`.

## TUI / status line

- **Always-on adaptive status line** (v0.87.1, extended v0.89.0): directory,
  branch, git counts, context, provider, model pinned below the input; compacts
  step by step on narrow terminals; `display.overscroll_status = "overscroll"`
  restores the old reveal behavior. Homes: `kcode-tui`, `kcode-config-types`.
- **Info widgets** (v0.89.0/v0.89.1): **Changes** widget (dirty files newest
  first, line counts, marks agent-edited files) and **Commits** widget (recent
  commits, line counts, unpushed marker); headers/legends on borders.
- **Reading position survives terminal resizes** (v0.89.0). `kcode-tui`.
- **Ctrl+C: first clears input, second quits** (v0.89.0). `kcode-tui` input.
- **Embedded and multiple slash commands in one message** (v0.89.0).
- **Terminal tab titles show session edit-line counts and elapsed time**
  (v0.86.0).

## Providers

- **Claude Opus 5.5 + independent Anthropic model discovery** (v0.87.0; default
  in v0.88.0): explicit API-key vs Claude-login routes, catalogs refresh
  independently, keep last good catalog on failure, cascade metadata, OAuth
  client bumped to Claude Code 2.1.280, adaptive thinking. Homes:
  `kcode-provider-anthropic*`, `kcode-provider-claude-cli-runtime`.
- **Cursor provider correctness** (v0.89.1, v0.89.2): native MCP bridge, live
  model-list resolution, `/fast`, tool results returned so MCP calls don't hang.
  `kcode-provider-cursor-runtime`.
- **Provider-native server-side web search** (`engine = "native"`, unreleased):
  prefer by default, respect session tool policy, label downgraded summaries.
  `kcode-app-core` websearch tool.
- **Additional providers**: OrcaRouter (v0.88.0), Yolo-Auto and grok-4.7/4.6 on
  Cursor (v0.89.0). Provider registry/config.
- **Provider fixes** (v0.89.0): Antigravity 429s from wrong endpoint, shared
  Claude usage limits across processes, DeepSeek/MiMo full context window,
  model-switch validation, effort chip follows model.
- **Anthropic prefill-repair fix** (unreleased): only skip for turns ending on a
  server tool block.
- **OpenAI WebSocket** (v0.86.0, unreleased): recover on changed tool outputs /
  missing tool result; end fresh streams on terminal error frames.

## Tools / agent behavior

- **`edit` does multi-replace atomically; retire the separate `multiedit`**
  (v0.88.0). Both exist here today (`kcode-app-core/src/tool/`).
- **MCP** (v0.86.0, v0.89.0, v0.89.1): provider-native deferred loading (no
  prompt-cache bust), preserve dynamic args, no startup stalls/duplicate spawns,
  pending requests fail when a stdio server exits.
- **Todo quality checks** (v0.86.0, v0.88.0): stop pushing ownership/confidence
  follow-ups for completed work and stay quiet after the final response.
- **Pre-tool input transformers** (v0.89.0). Hook/tool pipeline.
- **Scheduled tasks reach attached sessions**; **local client skills load before
  initial history**; **file-drop normalization** for non-image drops
  (v0.86.0). Homes confirmed: `kcode-tui` (`scheduled` handling, dropped-file
  input), `kcode-base` skill registry.
- **Edit results preserve file-position diffs**; distinguish a known-unchanged
  file from an unavailable snapshot (v0.86.0).
- **Harness API can move a running tool to the background** (v0.89.1). Optional;
  `src/bin/harness.rs` equivalent exists.
- **agentgrep 0.1.7 keeps partial results** when ripgrep hits unreadable files
  (v0.89.1).
- **Skill installs no longer invalidate the prompt cache** (unreleased).
- **Browser/Jev**: whole multi-step task handoff (v0.86.0); default-browser
  detection plus Chrome/Edge/Brave/Chromium/Safari (v0.88.0). `kcode-app-core`
  browser tool.

## Sessions / state

- **Saved sessions always visible; `/save` bookmarks persist** (v0.88.0).
- **Untitled sessions named after first prompt; save labels name the session**
  (v0.89.0).
- **Fresh-session history available during active turns; socket messages survive
  interrupted reads** (v0.86.0). `kcode-app-core` server.
- **Daemon reports KV cache misses to clients** (v0.89.0).
- **Credential import never overrides existing logins** (v0.88.0).
- **Auth badge updates immediately on OAuth <-> API-key switch** (v0.88.0).

## Usage / cost accounting

- **Cache-write tokens retained in usage reporting**; OpenAI cost no longer
  double-counts (v0.86.0).
- **Usage counts tokens from failed-mid-stream, crashed, compacted and
  background responses** (v0.89.3).
- **OpenAI banked usage resets** via confirmed command + availability hint
  (v0.88.0).
- **Account auto-switch pool with ordered failover** (v0.89.0) - uses the
  retained multi-account picker, not the removed jcode.sh account.

## Swarm

- **Agents keep model/provider/effort when their window attaches** (v0.88.0).
- **Cross-swarm messaging with unique labels** (v0.89.0).
- **Swarm dock**: per-agent status, current activity, todo progress (v0.89.1).
- **Long TLDR summaries accepted** (v0.88.0).
- `kcode-swarm-core`.

## Repo / maintenance

- **Source checkouts with no upstream skip misleading update checks**
  (v0.86.0). Directly relevant: this fork has no upstream.
- **`/merge`** integrates the current branch into the repo primary branch with
  validation of the combined result (v0.86.0). (The `-remote-release` variant is
  omitted: it assumes upstream CI.)
- **Optional external integration**: first-class herdr agent support (v0.89.3).
  Home exists in `kcode-terminal-launch`.
- **Ctrl+Up into prompt history keeps your draft**; null bash notification flags
  fall back to defaults; parallel tool calls stream names/inputs early;
  proxy tool arguments preserved until stream completion; interleaved OpenAI
  streaming tool calls stay matched (v0.88.0/v0.89.0).

## Suggested first batch

Highest value, lowest conflict, all with clear homes:

1. Adaptive status line + info widgets (self-contained in `kcode-tui`).
2. Opus 5.5 + Anthropic discovery (provider-local).
3. Provider-native web search engine (tool-local).
4. MCP deferred loading + lifecycle fixes.
5. Cursor provider correctness.
6. `edit` multi-replace unification.
7. Token-usage accounting fixes.
8. Update-check fix for upstream-less checkouts.
