# TUI render state ownership

Status: proposal, nothing implemented

The problem is not that the TUI has too few traits or too many modules. It is
that render state has no owner, so every renderer needs a wide key to reach it.
This plan fixes ownership and direction of flow. The traits, the wide interface
and most of the module sprawl fall out as consequences.

This supersedes the sub-trait split in
[tuistate-decomposition.md](tuistate-decomposition.md): if the data is owned and
threaded, `TuiState` has nothing left to abstract.

## Measured current state

| measurement | value |
|---|---|
| `pub trait TuiState` methods | 122 |
| `TuiState` implementors | 2 (`App`, `TestState`) |
| call sites taking `&dyn TuiState` | 49 |
| `App` fields | 315 |
| distinct `Pending*` structs | 11 |
| `pending_*` fields on `App` | 48 |
| process-global statics in `crates/jcode-tui/src/tui/` | 111 |
| of those, in `tui/ui.rs` | 37 |
| overlapping render caches | 3 (`body_cache`, `full_prep_cache`, message cache) |
| `crates/jcode-tui` size | ~181k lines; `tui/app/` alone is 157 files / ~114k lines |

The statics are the enabler. `tui/ui.rs` holds scroll offsets, viewport height,
visible copy targets and expand-badge placement in
`static X: OnceLock<Mutex<...>>`, and caches the derived transcript three times
at three layers. A renderer that draws one line therefore has to read from
`App` *and* from globals, which is exactly what a 122-method borrowed-`self`
interface is for. Remove the globals and the interface has no reason to exist.

## Target shape

One-way data flow, no globals read during draw:

```
Model          session, messages, providers, config          (pure data, no TUI)
  |
Transcript     messages -> prepared lines @ width, versioned  (the one cache)
  |
View           fn view(Model, Transcript, Ui, Interaction, Area) -> Frame   (pure)
  |
Paint          fn paint(&Frame, &mut Buffer)                  (dumb, stateless)
  ^
Interaction    Action -> State reducer; Effects back out to the runtime
```

Rules:

- `view` is a pure function from immutable inputs to a described frame. It can
  be computed without a `Terminal` and asserted on as a string.
- `paint` draws a frame and computes nothing.
- Input produces `Action` values; one reducer turns `(State, Action)` into state
  plus `Effect`s. Side effects leave as data.
- Nothing in the render path reads a global.

## Why this over a sub-trait split

Splitting `TuiState` into `TuiTranscriptState + ...` narrows bound declarations
but changes nothing about where data lives: `App` still implements the whole
surface, the two central renderers still need the supertrait, and the globals
stay. Owned state plus a pure `view` reaches the same goal (a module proves what
it reads) without adding 15 traits to maintain, and additionally makes the
render path snapshot-testable.

## Incremental steps

Each step compiles and ships on its own. Steps 1-2 are the unlock; stop after
them and most of the value is already banked.

1. **Own the render statics.** Move the `OnceLock<Mutex<...>>` render-state
   globals out of `tui/ui.rs` into an owned `ViewState` threaded through the
   render pass. Acceptance: no `static` holding render state remains in the
   render path.
2. **One cache, not three.** Fold `body_cache`, `full_prep_cache` and the
   message cache into one `Transcript` holding prepared lines plus a per-message
   version. Acceptance: one version counter drives all transcript invalidation.
3. **Delete `TuiState`.** Renderers take the concrete borrowed structs they
   already need (`&Transcript`, `&Header`, `&StatusBar`). All 122 methods, the 49
   `&dyn` sites and the supertrait question go with it.
4. **Split `App`.** Five owners: `Session` (exists), `Providers`, `Interaction`,
   `Connection`, `ViewState`. The 11 `Pending*` structs and 48 `pending_*` fields
   become one explicit state machine per concern.
5. **Make `view` pure and snapshot-tested.** Replace `TestBackend`-based pixel
   assertions with assertions on the described frame. This is what makes the
   pre-existing `jcode-tui --lib` failures tractable.
6. **Reorder the files to the layers** (`model/`, `transcript/`, `view/`,
   `paint/`, `interact/`) once the shapes are stable. Last, because it is pure
   churn until the boundaries are real.

## Keep

- ratatui.
- The crate seams that are already isolated components with clean inputs:
  `jcode-tui-markdown`, `jcode-tui-style`, `jcode-tui-render`,
  `jcode-tui-session-picker`, `jcode-tui-account-picker`. They are the only part
  of the tree already shaped like the target.

## Non-goals

- Not a compile-time or decoupling win. This is for correctness of state
  ownership and testability.
- Not a rewrite. Steps 1-5 are moves inside the existing crate; only step 6
  touches file layout, and it can be skipped.

## Verification

- `cargo check -p jcode-tui` after each step (`TMPDIR` must point at real disk,
  not RAM-backed tmpfs, or `ring`/`aws-lc-sys` build scripts fail with "Disk
  quota exceeded").
- `cargo test -p jcode-tui --lib` once per step. The lib suite has pre-existing
  parallel-order failures on `main`; verify any suspect in isolation with
  `--test-threads=1` before blaming the change.
- Step 1's acceptance check is a grep: no `static` holding render state in the
  draw path.
