# App Re-Core

Status: proposed
Owner: kcode
Audience: jcode-tui contributors

The `App` god module (`crates/jcode-tui/src/tui/app.rs`) is the single largest
cost in this codebase. This plan splits it into named sub-structs without
lowering capability, in stages that each land whole.

## The problem, measured

Sources: `graphify` (29,633-node graph) for the coupling hubs, then read from the
tree at `538b3636`:

- `struct App` spans `app.rs:821-1547`: **309 fields**, of which ~287 are loose
  scalars/options and ~22 are already sub-structs (`TokenAccounting`,
  `KvCacheState`, `StreamingProgress`, `CostState`, `OnboardingFlow`, ...).
- **57 `impl App` blocks across 53 files**, held together by **124
  `use super::*` globs**. `app.rs` itself is 2,471 lines; the impl blocks rank
  `inline_interactive.rs` 4,658, `input.rs` 4,176, `auth.rs` 3,028,
  `remote/key_handling.rs` 2,635.
- **6,182 `self.*` sites**, of which **~3,092 are direct `self.<field>` reads
  or writes**. That is the mechanical size of a full field re-home.
- **Test construction is the #1 hub in the graph.** `create_test_app()` has
  **771 edges**, more than `Provider` (374) or `Message` (266), and the largest
  community (277 nodes) is exactly that construction cluster. Every App shape
  change ripples through the test tree first, through one helper. Construction
  funnels are `App::new_minimal_with_session`, `new_for_replay*`, and
  `create_test_app`; grouping fields touches those, not call sites.
- **`TuiState` is a trait, not a struct** (`tui/mod.rs:335`), and it already
  names the target groups. 122 methods; exactly 2 impls: `App`
  (`tui_state.rs:537`) and `TestState` (`ui_tests/mod.rs:166`, a parallel
  39-field struct used in 83 sites across 13 files). `render_frame` takes
  `&dyn TuiState`, and ~77 render helpers take it too. Its cost is the wide
  interface plus the duplicate `TestState`; its benefit is rendering the UI in
  tests without constructing an `App`. See Stage 11: deleting it is a trade,
  not free.
- The runtime axis is encoded **three times**: `runtime_mode: AppRuntimeMode`
  (24 uses), `is_remote: bool` (129 real accesses), `is_replay: bool` (~16),
  plus `is_remote_mode()` / `is_replay_runtime()` accessors that re-derive the
  same thing. All three are set together at construction
  (`tui_lifecycle.rs:535,926`, `tui_lifecycle_runtime.rs:16-18`), so the two
  bools are derivable and the illegal combinations are currently expressible.
- `app.rs` is already under the size ratchet
  (`scripts/code_size_budget.json`, threshold 1200). It can only shrink.

Access-site counts by candidate group (the cost driver for each stage):

| group | fields | `self.field` sites |
|---|---|---|
| Stream/Status | 26 | 414 |
| Session/Server | 18 | 390 |
| Input | 17 | 303 |
| Provider | 15 | 281 |
| Overlay (pickers/login) | 14 | 174 |
| Transcript | 13 | 170 |
| Scroll/Render | 13 | 145 |
| Accounting ctx | 10 | 133 |
| Copy/Select | 9 | 103 |
| Onboarding | 14 | 90 |
| SidePanel/Split | 12 | 89 |
| Swarm panel | 13 | 88 |
| Todos | 14 | 72 |
| (121 ungrouped loose fields) | 121 | 640 |

## Target state

`App` becomes a coordinator: a list of named sub-structs plus a small scalar
tail. Each sub-struct is also the `impl` home for the behavior that touches only
it, so a change to scroll behavior is read in `Viewport`, not `App`. The pattern
already exists in-tree and is the model to copy: `impl OnboardingFlow`
(`onboarding_flow.rs`), `impl RemoteLogin` (`auth_remote.rs`),
`impl CopyBadgeUiState` (`app.rs:489`).

**Group names come from the tree, not from this document.** The `TuiState`
trait already partitions the surface, and its sections are the canonical names:
Transcript, Input, Scroll, Provider, Stream/status, Session/server, Workspace,
Diff pane, Side panel, Inline, Overlay, Copy selection, Onboarding, Misc. The
stages below use those names where they overlap and add only what the trait does
not cover (Todos, Panels, Swarm, Accounting).

Three properties fall out:

- **One obvious home per concept.** Fields and the methods over them share a
  type and a file.
- **Legible blast radius.** A group's access sites become `self.<group>.field`;
  the group's file is the whole surface.
- **A small core.** `App` keeps only cross-group orchestration (turn
  submission, render loop, event dispatch) and stops being the namespace for
  everything.

## The move, and why grouping fields alone is not it

Renaming `self.scroll_offset` to `self.viewport.offset` across 145 sites is pure
churn if nothing else changes. The payoff comes only when the **methods move
with the fields**: `impl App`'s copy-selection methods become
`impl CopySelection`, input's become `impl Input`, and so on. That is what
drains the 57 impl blocks and the 124 globs.

So the ordering is by **isolation first, coupling last**:

- Pure-state groups (copy selection, viewport, todos, panel toggles) can move
  their methods onto the sub-struct with no plumbing. Do these first; they prove
  the template cheaply.
- Coupled groups (input, stream/status, provider, session/server) have methods
  that orchestrate across groups and may need `&mut App` passed in. They come
  last, once the isolated moves have shrunk the surrounding surface.

Estimation is deliberately generous: the field and site counts above are
measured, but **which methods are cohesive enough to move is not yet measured**.
Task 0 settles that per group, and a group whose methods are not cohesive is a
stop-and-report, not a forced extraction.

## Stages

Each stage is an independent, complete change. Do not start a stage you will not
finish; half-migrated is the most expensive state.

### Stage 0 - Baseline and shape ratchet

- Record the `cargo test -p jcode-tui` baseline (todo.md notes 30 pre-existing
  `--lib` failures; confirm the count before touching anything).
- Add a small ratchet, in the shape of the existing `check_*.py` scripts:
  `scripts/check_app_shape.py` counting `struct App` fields and `impl App`
  blocks per file, `--update` to rebaseline. The counts only go down. This is
  what keeps the pass monotonic across stages instead of regressing on the next
  feature.
- Per group, measure cohesion: for each `impl App` method body, does it
  reference fields from one group only? That list decides which methods move in
  each stage.

### Stage 1 - One runtime axis

Delete the duplicate encodings; keep `runtime_mode`.

- Remove `is_remote: bool`, `is_replay: bool`, `replay_elapsed_override`'s
  sibling bool if redundant. Keep `runtime_mode: AppRuntimeMode`.
- Replace field reads with the existing `is_remote_client()` /
  `is_replay_runtime()` (or thin `is_remote()` / `is_replay()` if the call
  sites read better).
- Test writes `app.is_remote = true` become a helper
  `app.set_runtime_mode(AppRuntimeMode::RemoteClient)`; production only sets it
  in `new_for_replay_with_title` and `new_minimal_with_session`.
- Verify: `cargo test -p jcode-tui`, plus a grep proving no remaining field
  assignment.

**Concept deleted:** three encodings of one axis. Illegal combinations become
unrepresentable. This is the cheapest stage and it shrinks every later one.

### Stage 2 - CopySelection (template stage)

- Extract `struct CopySelection` from the 9 copy fields (103 sites), keeping
  `CopyBadgeUiState` nested as-is.
- Move `copy_selection.rs`'s `impl App` methods to `impl CopySelection`. Methods
  that need the viewport take `&mut` on the viewport or stay on `App` if they
  read more than one group.
- Home: `copy_selection.rs`.

Smallest isolated group. If the template does not read well here, fix the
approach before spending it on a bigger group.

### Stage 3 - Viewport / scroll

- Extract `struct Viewport`: `scroll_offset`, `auto_scroll_paused`,
  `scroll_bookmark`, `typing_scroll_lock`, `overscroll_status_mode`,
  `chat_native_scrollbar`, `force_full_redraw`, `force_full_repaint`,
  `resize_redraw_pending`, `last_resize_redraw`, and the `*_scroll` fields (13
  fields, 145 sites).
- Home: `redraw_schedule.rs` / `navigation.rs`.

### Stage 4 - TodosView

- Extract `struct TodosView` from the 14 `todos_view_*` / `todo_*` /
  `pinned_todos_*` fields (72 sites).
- Home: `todos_view.rs` (783 lines).

### Stage 5 - Panel and split-view state

- Extract `struct Panels` for the side-panel/split fields (12 fields, 89 sites)
  including `side_pane_ratio*`, `side_panel_*`, `split_view_*`.
- Home: `split_view.rs`.

### Stage 6 - Overlay / picker state

- Group the four overlays that already have homes: session picker (14 fields,
  174 sites, `session_picker.rs`), model picker (`model_context.rs`), account
  and login pickers (`auth_*.rs`). Each becomes its own sub-struct owning its
  `impl` block.
- This is where the `use super::*` globs start disappearing: `session_picker.rs`
  stops reaching into `App` and operates on `SessionPicker`.
- Home: the picker file.

### Stage 7 - Stream and status

- Extract the streaming/turn-progress core (26 fields, 414 sites). `streaming`,
  `stream_buffer`, `streaming_md_renderer`, `status`, `is_processing`,
  `thinking_*`, `reasoning_*` cluster around one lifecycle.
- Home: `turn.rs` / `state_ui_runtime.rs`. Expect cross-group reach here; some
  methods legitimately stay on `App`.

### Stage 8 - Provider and model context

- Extract provider identity + model context (15 fields, 281 sites). Coordinate
  with the provider-identity unification item in `todo.md`; do not create a
  second provider registry here.

### Stage 9 - Input

- Extract `struct Input` (17 fields, 303 sites; `input.rs` is 4,176 lines).
  Largest single drain of `impl App`. The input methods that submit turns or
  switch models take `&mut App`; the pure editing/undo/queue state moves wholly.

### Stage 10 - Transcript and session/server

- Last, because they are the most coupled. Transcript (13 fields, 170 sites,
  `conversation_state.rs`), then the session/server block (18 fields, 390
  sites). Sweep the remaining ~121 loose fields into whichever group owns them;
  anything that resists grouping is itself a finding (a hidden concept or a
  coupling to name).

### Stage 11 - Retire `TuiState` (trade, decide last)

Not the free win it looks like. `TuiState` is a 122-method trait whose second
impl, `TestState`, exists so render tests can run without constructing an `App`
(83 sites, 13 files).

- The win: one interface in `mod.rs`, a 39-field duplicate struct, and ~77
  `&dyn TuiState` helpers collapse to `&App`.
- The cost: those 83 sites must build an `App` instead, through
  `create_test_app` - already the graph's #1 hub. That is why this stage is
  last: thinning `App` (Stages 2-10) is what makes constructing it cheap enough
  for the trade to pay.
- Decide with the trade on the table, not by assuming the trait is vestigial.
  If `TestState` still earns its keep after `App` is thin, keep the trait and
  delete only the duplicate surface.
- Either way, update the trait's doc comment (`tui/mod.rs:325-334`), which
  currently points at `docs/todo.md`.

## Out of scope

- The `handle_client` split and the local turn path are separate items in
  `todo.md`. They share the "god module" theme but different files; keep them
  out of these diffs.
- Provider identity, command-surface unification, and the crate spine are their
  own todo items. Staging touches their seams (Stage 8 especially) but does not
  absorb them.

## Risks

- **Churn without payoff.** If a stage's methods turn out to be cross-group, the
  extraction is a rename, not a drain. Task 0 detects this; skip or re-scope.
- **God module by another name.** A sub-struct that grows a flag per caller is
  the tripwire. Groups here are state shapes with a small method set, not
  services; keep behavior local and the group small.
- **Half-migration.** The shape ratchet from Stage 0 exists to make the
  monotonic direction visible.

## How we know it worked

- `struct App` field count and `impl App` filename count fall monotonically;
  `check_app_shape.py` enforces it.
- `grep -rn "use super::\*" | wc -l` falls from 124 as stages land.
- `app.rs` leaves `code_size_budget.json` (drops below 1200 lines).
- A change to scroll behavior is read in `Viewport` alone; a change to a picker
  is read in its picker file alone.
- Re-run `graphify god_nodes`: `create_test_app` should lose its #1 slot and the
  construction community should shrink as `App` stops being the only way to
  express the state the renderer reads.

## How this was measured, and what is still a guess

Measured: field count and span, `impl`/glob/site counts, per-group access-site
costs, the `TuiState` impls and `TestState` usage, and the coupling hubs
(`create_test_app` 771 edges, community 0) - via `graphify query`/`god_nodes`
plus reading `app.rs`, `tui/mod.rs`, and `ui_tests/mod.rs`.

Still unmeasured: **whether a group's methods are cohesive enough to move onto
it.** The graph models type references, not field-to-method edges, so it cannot
answer this; it needs reading each `impl App` body. Stage 0 does that per group.
A group whose methods span groups gets re-scoped; an estimate here would be a
guess dressed as a finding.
