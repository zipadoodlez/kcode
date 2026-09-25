# Work in progress

The index of what is not finished, so it does not get lost between passes. This
file is expected to shrink.

When an item lands, delete its row. When a plan completes, delete the plan doc -
git has the history.

## Plans in flight

| plan | state | what's left |
|---|---|---|
| [plans/limited-palette.md](plans/limited-palette.md) | phase 1 of 4 done (ratchet guard ported, 697 literals / 30 files) | add `[display.palette]` (16 slots) and role→slot defaults, `/colors` edits slots; then collapse the 697 literals family by family until `BASELINE` is empty |

## Committed ideas, no plan doc yet

| idea | the problem |
|---|---|
| compile-time isolation | build and link time; the workspace recompiles far more than it should |
| `TuiState` decomposition | `TuiState` is now **156 methods**, up from the 114 the old doc measured |
| onboarding invariants | onboarding state invariants are enforced by review, not CI |

## Open items in the code, no plan

| item | state |
|---|---|
| `JCODE_*` env vars | the state dir is `~/.kcode` but the env prefix was never renamed. Decide: rename with a `JCODE_*` fallback, or document as-is |
| dead SSH-block commands | `/theme`, `/stats`, `/file`, `/open`, `/permission`, `/permissions`, `/new-terminal`, `/debug-fixture` are blocked over SSH but have no handler anywhere, so they do nothing locally either |
| `client:mermaid:*` debug help | `server/debug_help.rs` still lists ~9 mermaid/image debug commands after the mermaid removal. Some (`mermaid:ui-bench`) have a tester handler, so this needs verifying, not a blind delete |
| `-p` vs `provider list` | `-p` accepts 52 provider choices; `provider list` prints 26. The extras are aliases and gateways with no catalog entry |
| `/help <item>` coverage | written detail for 70 of 114 registered commands; the other 44 answer `Unknown command`, and there is no `/help list` |
| packaging | `packaging/arch/PKGBUILD` does not exist, and the README install section points at it |
| unknown config sections | the loader silently ignores unknown top-level sections, so older config files keep dead keys with no warning |

## Hook surface gaps

Found while writing `user/hooks.md`.

| gap | detail |
|---|---|
| `turn_start` `SOURCE` | only `chat` is ever emitted; the schema advertises `chat`/`resume`/`ambient`, and `ambient` belongs to a removed mode. Narrow the contract or wire the resume path |
| `session_start` schema comment | stale: says `create`/`resume`; the code emits `create`, `attach`, `resume` |
| hooks are unobservable | no `/hooks` command, no listing of configured hooks, no dry-run. A typo in a command string looks identical to a hook that runs and does nothing |
| blocked calls are invisible | `pre_tool` stderr goes to the model; nothing tells the user their policy blocked a call |
| hook failures | logged and dropped, with no user-visible signal |

## This doc set

The rebuild is in progress:

- **Done**: `README.md` (index), `what-was-removed.md`, `message-voice.md`,
  `plans/limited-palette.md`, `user/hooks.md` (folded `HOOKS.md` +
  `SPAWN_HOOK.md`), `user/auth.md` (from `AUTH_CREDENTIAL_SOURCES.md`),
  `user/ssh.md` (from `NATIVE_SSH.md`), `user/providers.md` (folded
  `AWS_BEDROCK_PROVIDER.md` + `CONIFER_PROVIDER.md` + `PROVIDER_DOCTOR.md`),
  `internals/usage.md` (folded `MODEL_USAGE.md` +
  `CHATGPT_API_EQUIVALENT_USAGE.md`), `user/tui.md` (folded `SHIFT_ENTER.md` +
  `RESUME_BEHAVIOR.md`), `user/cli.md` (from `WRAPPERS.md`), `user/config.md`
  (from `SYSTEM_PROMPT_CONFIG.md`), `internals/memory.md` (folded
  `PROCESS_MEMORY_BUDGET.md` + `PROCESS_MEMORY_INCIDENT_RUNBOOK.md`),
  `internals/rendering.md` (folded `TUI_COLOR_CONFIGURATION.md` +
  `TERMINAL_CAPABILITIES.md`), `internals/architecture.md` (folded
  `SERVER_ARCHITECTURE.md` + `MULTI_SESSION_CLIENT_ARCHITECTURE.md` +
  `HERDR.md` + `PANELS.md`), `internals/swarm.md` (folded
  `SWARM_ARCHITECTURE.md` + `SWARM_TASK_GRAPH.md`).
- **Dropped as unshipped/speculative** (git has it): the protocol-multiplexing
  phases and open questions from `MULTI_SESSION_CLIENT_ARCHITECTURE.md` (the
  client split and workspace map shipped, so only the prose survived into
  `internals/architecture.md`), and the upstream Herdr TODO list.
- **Not written**: the rest of what the index promises under `user/` and
  `internals/`.
- **Install stays in the root README**: no `user/install.md`. The `## Install`
  section there is already embedded in the corpus, and a second copy would drift
  (and would only repeat that packaging is unfinished).
- **Repo-root doc folded**: `OAUTH.md` (per-provider login and troubleshooting)
  is now `user/providers.md` + `user/auth.md`.
  `openai_docs_reference_current_callback_uri` was repointed from
  `OAUTH.md`/`README.md` to `docs/user/providers.md` (it had been failing, since
  README never carried the callback URI).
- **Pending**: 14 legacy docs still sit at the `docs/` root in `SCREAMING_CASE`,
  awaiting rewrite into the new set or deletion.
- **Code references docs that move**: `tui/mod.rs` names
  `docs/TUISTATE_TRAIT_DECOMPOSITION.md`; update it when
  `plans/tuistate-decomposition.md` lands.

## Plans that look finished

These appear to describe shipped code. Confirm, then delete them - a plan for
finished work is drift.

- `plans/MCP_SKILLS_PLAN.md` - MCP client and skill hot-reload both exist.
- `plans/OPENAI_COMPATIBLE_PROFILE_RUNTIME_PLAN.md` - named OpenAI-compatible
  profiles exist in the config types and provider code.
