# Pre-spec: Keel

> **Not a spec for kcode.** This document is musing about a possible *future*
> harness, written after comparing kcode, maki, and grok-build. It is not a plan,
> a lane, or a commitment for kcode, and nothing here should be read as describing
> kcode or its direction. It lives in `docs/plans/` only because that tree is kept
> out of the bundled `kcode_docs` corpus, so the model never serves it as a
> description of the tree.

A minimal agent harness that keeps the best ideas of kcode, maki, and grok-build
while carrying the least surface of the three. Status: draft pre-spec, no code.
"Keel" is a placeholder name.

## 1. Thesis

A harness is a policy system wrapped around a small turn loop. Everything that is
not the loop is either **data** (a policy) or a **plugin** behind one boundary.
Surface is the enemy. A new capability must be bought with a policy slot or a
plugin, never with a new crate or a new subsystem.

The three references disagree on where to spend surface:

- **maki** spends it on an in-process scripting layer (32k Lua, 57k Rust host).
- **kcode** spends it on breadth (60 provider/service crates, a 154k-line TUI).
- **grok-build** spends it on a product (2.0M lines, a monorepo, vendor coupling).

Keel's bet is that the *decomposition* is the value, not the volume.

## 2. Non-goals

- Not a monorepo product. One workspace, no generated manifests, no DotSlash.
- Not a model vendor's client. No backend-hosted tools, no server-side sampler.
- Not a provider catalog. Providers are config, not crates.
- No in-core code graph, LSP, or embeddings. Structure comes from an external tool.
- No per-capability transport crates and no per-provider runtime crates.
- No second language for the UI.

## 3. Laws

The axioms that keep the surface small. Each is enforceable.

1. **Policies are data.** Compaction, reminders, permissions, budgets, retries,
   and model routing are plain config structs. No policy is spread across modules.
2. **One boundary, dogfooded.** Built-in tools are plugins. If the plugin API
   cannot express a built-in, the API is wrong, and that is the point.
3. **One of each.** One wire, one store, one script engine, one renderer, one
   provider transport. Adding a second of any of these requires deleting the first.
4. **Spawn, do not vendor.** Prefer an external binary over a compiled grammar or
   a vendored dependency. (This is the ctags-over-tree-sitter argument, generalized.)
5. **Deletion is the default step.** A change that only adds surface is rejected
   before it is written. The ratchet enforces it.
6. **Extension points are the only stable API.** Core internals are private and
   may change freely. Users depend on the boundary, never on the core.

## 4. The core

The irreducible centre. Everything else is a policy or a plugin.

- **Turn loop.** One async loop: assemble prompt, call model, dispatch tools,
  append, compact if needed, repeat. No modes beyond interactive and headless.
- **Session model.** Messages, tool calls, and events as one append-only log. A
  session is a replayable value, which makes resume and orchestration free.
- **Policy slots.** `PromptPolicy`, `ReminderPolicy`, `CompactionPolicy`,
  `PermissionPolicy`, `BudgetPolicy`. Populated from config, read at fixed points
  in the loop.
- **The plugin host.** One boundary (see section 7). Registers tools, providers,
  hooks, and display specs.
- **The wire.** One protocol (ACP), served and consumed. Headless mode is a thin
  shim over it, not a third format.
- **The store.** One SQLite database for sessions, memory observations, and run
  journals. One journaling mechanism, reused.

## 5. Subsystems, each at its floor

### 5.1 Context

- **Take:** grok's two-pass speculative compaction, a memory-flush turn before
  compaction, and a wall-clock budget as the backstop. maki's discipline that
  prompt and tool descriptions are short.
- **Floor:** one token estimator as the single source of truth (bytes/4), used by
  the compaction gate, the preflight overflow check, and every renderer. Compaction
  is one policy plus one renderer.
- **Out:** a code-execution trampoline. The evidence (grok has none) says compaction
  plus provider-side search beats a scripted tool host for context pressure.
- **Trade:** runtime system reminders with gates and nudges, driven off todo and plan
  state, instead of a large static prompt.

### 5.2 Tools

- **Take:** grok's output verbosity variants (concise, hashline), tool result caps,
  and plan-mode tools. kcode's `intent` field and withheld-large-output escape hatch.
- **Floor:** a tiny tool trait (name, schema, execute, optional display spec). A
  standard library of roughly a dozen tools, all implemented as plugins.
- **Out:** LSP, image/video generation, app deployment, scheduler, monitor. Those are
  plugins if a user wants them, never core.
- **Structure:** file structure comes from one external `ctags` call, exactly as
  kgrep does today, for both per-file outline and a repo-wide index.

### 5.3 Safety

- **Take:** grok's layered model, which is the matured form of kcode's
  `command-risk`. A standalone permission engine (bash command splitting, exec risk,
  env risk, folder trust, managed policy) plus OS enforcement.
- **Floor:** two layers, in order. Layer one is deterministic classification by blast
  radius. Layer two is OS sandboxing (Landlock/Seatbelt) applied at startup, with
  per-child network blocking. Catastrophic paths are an absolute deny that does not
  depend on parsing.
- **Out:** a denylist of command names. Explicitly rejected.
- **Also:** SSRF protection on `webfetch`, re-checked after redirects.

### 5.4 Orchestration

- **Take:** grok's journaled run engine with deterministic replay, and its typed
  subagent messaging (`queue` / `steer` / `interject`, parent/peer routing), plus
  agent budgets. kcode's swarm intent.
- **Floor:** runs are scripts over the session log. A run is journaled, so it is
  resumable and inspectable. Budgets are a policy. No bespoke DAG types in core.
- **Out:** a second orchestration model beside the plugin engine. Runs use the same
  engine as plugins (section 6).

### 5.5 Memory

- **Take:** grok's v2 shape, which is the answer to the drift problem. An immutable
  observation inbox, consolidation, and a generated manifest. Explicit supersede and
  recency, so a reversed decision is invalidated rather than re-read.
- **Floor:** observations are rows in the one store. Consolidation writes one
  manifest. Retrieval is by tag with recency surfaced.
- **Out:** model-maintained free-form state (maki's files, kcode's deleted tool).
  Durable memory is either user-authored or consolidated, never raw model scratch.

## 6. One engine, two jobs

The single largest surface decision. User plugins and orchestration runs share
**one** sandboxed script engine (WASM via Extism is the default candidate, a small
Rhai or Lua embed is the alternative).

- One runtime, one permission model, one host API, one set of tests.
- A user tool and a workflow are the same kind of object.
- The engine is out-of-the-hot-path: no per-frame rendering through it.

If this proves too clever, the fallback is to drop the workflow engine and keep only
user plugins, which is strictly less surface.

## 7. The extension boundary

One boundary, three things it can register, all dogfooded:

1. **Tools.** Schema, execute, and an optional **declarative display spec**
   (title, collapsible body, status line, diff view). This replaces maki's Lua UI
   layer at a fraction of the cost and makes MCP-style tools feel first-party.
2. **Providers as data.** A provider is a config entry (transport kind, base URL,
   auth, catalog). No per-provider crate. Non-OpenAI wire shapes are adapter data,
   not new subsystems.
3. **Hooks.** A small fixed set (before turn, after tool, after write, on compact).
   This covers the "format after write", "retitle session", "notify" plugin class
   without a scripting runtime.

The boundary is the only stable API. It is exercised by built-ins from day one.

## 8. What we drop, and from whom

| Idea | Source | Keep | Why |
|---|---|---|---|
| Per-file tree-sitter index | maki | No | Replaced by external ctags outline plus repo index |
| Code-execution trampoline | maki | No | Compaction plus provider search is cheaper (grok evidence) |
| Lua plugin UI layer | maki | No | Declarative display spec covers most of it |
| Model-written memory files | maki | No | Drifts on direction change; use observations plus consolidation |
| Codebase graph (tree-sitter) | grok | No | Spawn ctags instead; the 54 MB of grammars is the wrong cost |
| Vendor-hosted tools, sampler | grok | No | Couples the harness to one backend |
| Monorepo scale, generated manifests | grok | No | The scale is the liability, not the achievement |
| Managed policy, folder trust | grok | Yes | Enterprise needs are real and small |
| OS sandbox with child net block | grok | Yes | The actual answer to destructive commands |
| Two-pass compaction, memory flush | grok | Yes | Long-session quality for one policy |
| Journaled run engine | grok | Yes | Resumable orchestration, and it is the plugin engine too |
| Provider-crate sprawl | kcode | No | Collapse to one compat transport plus adapter data |
| God `App` and `handle_client` | kcode | No | Named policy structs instead |
| Swarm, browser, panels | kcode | Yes, as plugins | Keep the capability, move it out of core |
| Command-risk by blast radius | kcode | Yes | Correct instinct, moved into the permission engine |

## 9. Surface budget

Targets, enforced by a ratchet file, not by intent. Compare with maki's ~249k code
lines and kcode's ~475k.

| Part | Target |
|---|---|
| Core loop, session, policies, plugin host, wire, store | 25k |
| Context and compaction | 12k |
| Tool standard library | 20k |
| Safety (permission engine plus OS sandbox) | 12k |
| Orchestration and run engine | 15k |
| Memory and consolidation | 8k |
| Providers (one transport plus data) | 12k |
| TUI | 45k |
| **Total** | **~150k** |

Also ratcheted: crate count, dependency count, binary size, and cold startup. The
budget is measured per capability, not only in total, so a subsystem that grows
without a matching capability is visible.

## 10. Risks and open questions

- **One engine for plugins and workflows** may be over-clever. Fallback is plugins
  only, which is less surface.
- **A declarative display spec** may not express something like maki's `/review`
  inline comments. If it cannot, that is a signal the spec is too weak, not that the
  core needs a scripting UI.
- **Spawn, do not vendor** adds runtime dependencies (ctags, sandbox primitives) that
  packaging must own.
- **WASM sandbox cost** against in-process speed for tool dispatch, unmeasured here.
- **Dogfooding the boundary** slows early tool work. That cost is deliberate.

## 11. Success criteria

- Core stays under ~150k lines with the ratchet green.
- Every new capability is a policy or a plugin, never a crate.
- Built-in tools are implemented as plugins.
- A user adds a tool, a provider, or a hook by editing config or dropping one plugin,
  with no recompile.
- A long session stays under the context budget through compaction alone.
- A destructive command is stopped by OS enforcement, not only by classification.

## 12. Next steps

1. Choose the script engine, or decide to ship plugins-only for v0.
2. Prototype the plugin boundary and dogfood the `read` tool through it.
3. Write the five policy structs and read them from config.
4. Stand up the external `ctags` outline path.
5. Land the OS sandbox layer under the permission engine.
