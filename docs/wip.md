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
| `-p` vs `provider list` | `-p` accepts 52 provider choices; `provider list` prints 26. The extras are aliases and gateways with no catalog entry |
| `/help <item>` coverage | written detail for 70 of 114 registered commands; the other 44 answer `Unknown command`, and there is no `/help list` |
| packaging | `packaging/arch/PKGBUILD` does not exist, and the README install section points at it |
| unknown config sections | the loader silently ignores unknown top-level sections, so older config files keep dead keys with no warning |

## This doc set

The rebuild is in progress:

- **Done**: `README.md` (index), `what-was-removed.md`, `plans/limited-palette.md`.
- **Not written**: everything the index promises under `user/` and `internals/`.
- **Pending**: 40 legacy docs still sit at the `docs/` root in `SCREAMING_CASE`,
  awaiting rewrite into the new set or deletion.

## Plans that look finished

These appear to describe shipped code. Confirm, then delete them - a plan for
finished work is drift.

- `plans/MCP_SKILLS_PLAN.md` - MCP client and skill hot-reload both exist.
- `plans/OPENAI_COMPATIBLE_PROFILE_RUNTIME_PLAN.md` - named OpenAI-compatible
  profiles exist in the config types and provider code.
