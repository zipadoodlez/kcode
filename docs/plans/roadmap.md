# Roadmap

Status: proposed
Owner: kcode
Audience: anyone picking up `docs/todo.md`

`todo.md` is the checklist. This is the order to work it in, and why that order.
Designs live in `plans/<item>.md`; this file only sequences them.

## Why an order at all

Some items are prerequisites: a few shared representations sit under many call
sites, so fixing the shape first collapses work that would otherwise be redone.
Others are decisions that block work outright. Working the list top-to-bottom
instead would migrate call sites against a shape we are about to change.

## The core representations, from the graph

`graphify god_nodes` on the current tree ranks the shared shapes by fan-in. The
backlog is not evenly weighted; it is weighted like this:

| node | edges | item it belongs to |
|---|---|---|
| `create_test_app()` | 771 | test construction (App re-core ripple + tests family) |
| `Provider` | 374 | provider identity |
| `Message` | 266 | protocol core (stable, not on the list) |
| `ServerEvent` | 214 | protocol core (stable) |
| `TuiState` | 206 | App re-core |
| `SwarmMember` | 182 | swarm condensation |
| `App` | 137 | App re-core |
| `ModelRoute` | 128 | provider identity |
| `LoginProviderDescriptor` | 126 | provider identity |
| `RemoteConnection` | 125 | server/protocol |

So four backlog families carry the real fan-in: **provider identity**, **App /
TuiState**, **swarm**, and **test construction**. They are also the four most
expensive to defer, because every feature written before them lands on the old
shape.

## The families

Each family is a coherent pass. Items inside a family share files and a root
cause; do not interleave families within one change.

- **A. TUI state shape.** Re-core `App` (`plans/app-recoring.md`), retire or
  keep `TuiState`, remove the orphaned local turn path.
- **B. Provider identity.** One `ProviderId` registry; derive the seven enums
  plus the string vocabularies. Keep every provider.
- **C. Server god module.** Split `handle_client`; introduce the request-context
  struct; retire the stringly command surface.
- **D. Swarm.** Condense `SwarmState`, unify the member projection (currently
  hand-written 4x) and the status vocabulary.
- **E. Spine.** Wildcard reexports to zero, crate spine, dependency diet,
  compile-time isolation. One direction, four items.
- **F. Tests.** Replace the `include!` tree with real modules, condense
  near-duplicate tables, move subsystem code out of test files.
- **G. Hygiene and docs.** Emitted env prefix, config sections, ambient
  vocabulary, selfdev package names, `scripts/` triage, packaging.
- **H. Decisions.** Quality ratchets, fork policy.

## The order

### Phase 0 - Decisions and baseline (cheap, unblocks the rest)

1. **H: fork policy.** Rebase lane vs hard divergence decides crate names and the
   `JCODE_*` prefix, which several items wait on. Settle it first.
2. **H: quality ratchets.** Drop the swallowed-error ratchet; re-baseline or drop
   the panic ratchet. Independent, and it makes the gate trustworthy.
3. **A: baseline + shape ratchet** (`plans/app-recoring.md` Stage 0). Records the
   test baseline and starts the monotonic counter.
4. **G: `scripts/` triage.** ~80 inherited files. Cheap, reduces noise in every
   later sweep.

### Phase 1 - Shared shapes (deep, collapses call sites)

Order within the phase is by fan-in, so the widest shape moves first while the
tree is still quiet:

5. **B: provider identity.** The widest representation and it spans crates
   (`ActiveProvider`, `RuntimeProviderId`, `ProviderChoice`, `ModelRoute`,
   `Provider`, `LoginProviderDescriptor`). Migrate on change, do not sweep the
   153 string sites first. This also settles `-p` 52 vs `provider list` 26.
6. **D: swarm condensation.** `SwarmState` is a real owner; condense the member
   projection and status vocabulary. Do this before C, because the
   request-context struct is designed to hold `SwarmState`.

### Phase 2 - The two god modules (staged, land whole)

7. **A: re-core `App`**, stages 1-10 of `plans/app-recoring.md`. Isolated groups
   first, coupled ones last. Command-surface unification (C) shares
   `commands_dispatch.rs`, so keep them in separate changes.
8. **C: split `handle_client`** and retire the stringly command surface. Now the
   request-context struct can hold a settled `SwarmState` (D) and a settled
   provider identity (B).

### Phase 3 - Spine

9. **E:** wildcard reexports to zero, then the crate spine, then dependency diet,
   then compile-time isolation. Each depends on the previous being settled, and
   all of them are easier once A and C have shrunk the cross-crate surface.

### Phase 4 - Tests

10. **F: replace the `include!` tree with real modules.** Do this after the shape
    work, not before: the test tree is coupled through `create_test_app` (771
    edges), so shape changes are currently paid for in test churn. Replacing the
    include tree first would just move that churn around.
11. **F: condense near-duplicate tables and move subsystem code out.**

### Phase 5 - Hygiene and packaging

12. **G:** the remaining hygiene items in a batch (`JCODE_*` per the fork
    decision, config-section warnings, ambient vocabulary, selfdev package
    names), then packaging/`PKGBUILD`.
13. **A: retire or keep `TuiState`** (`plans/app-recoring.md` Stage 11). Last,
    because it is a trade that only pays once `App` is thin enough that building
    one in a test is cheap.

## Decision gates

These block work and need a maintainer call, not more analysis:

- Fork policy (blocks crate names, `JCODE_*` prefix, packaging).
- Quality ratchets (swallowed-error, panic).
- SSH-login state re-core: confirm before touching credential code.
- Packaging: write `packaging/arch/PKGBUILD` or drop the README promise.

## Measured vs judgment

Measured: the god-node fan-in above, the App field/site counts and `TuiState`
impls (`plans/app-recoring.md`), and the per-item facts recorded inline in
`todo.md`.

Judgment: the phase ordering. It rests on one claim - fix the shared shape before
sweeping call sites - and on the dependency notes (C needs D's `SwarmState`, F is
safer after A). If a dependency turns out to point the other way when you read
the code, take the local order over this document.
