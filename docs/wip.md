# Work in progress

The index of what is not finished, so it does not get lost between passes. This
file is expected to shrink.

When an item lands, delete its row. When a plan completes, delete the plan doc -
git has the history.

## Plans in flight

| plan | state | what's left |
|---|---|---|
| [plans/limited-palette.md](plans/limited-palette.md) | phase 1 of 4 done (ratchet guard ported, 697 literals / 30 files) | add `[display.palette]` (16 slots) and role→slot defaults, `/colors` edits slots; then collapse the 697 literals family by family until `BASELINE` is empty |
| [plans/tuistate-decomposition.md](plans/tuistate-decomposition.md) | analysis only, nothing extracted | refresh the 114-method categorization to the current 156, then extract leaf sub-traits one per commit starting with a single-file consumer, keeping `ui.rs` and `ui_viewport.rs` on the supertrait |
| [plans/browser-provider-protocol.md](plans/browser-provider-protocol.md) | draft spec, no implementation | tighten the core method set and the normalized `page.snapshot` format before building any adapter |

## Committed ideas, no plan doc yet

| idea | the problem |
|---|---|
| compile-time isolation | build and link time; the workspace recompiles far more than it should. The old `COMPILE_TIME_ISOLATION_REFACTOR.md` was a historical record naming removed machinery, so it was deleted - the idea stays here |

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

The rebuild is complete: `user/` (7), `internals/` (9), `dev/` (4), `plans/` (3),
plus `README.md`, `what-was-removed.md`, and this file at the root. Notes that
outlive it:

- **Install** stays in the root `README.md`; there is no `user/install.md`.
- The repo-root `OAUTH.md` is folded into `user/providers.md` + `user/auth.md`.
- **Dropped as unshipped/speculative** (git has it): the multi-session
  protocol-multiplexing phases and open questions, and Herdr's upstream TODO list.
