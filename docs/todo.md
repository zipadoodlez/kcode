# Todo

Open work, in the order that unblocks the most: every item is something to do, or a
call to make. The design lives with the item when it is short and in `plans/` when it is
not; a landed decision is a record, not work, and lives in `what-was-removed.md`. See
`README.md` for the conventions.

## 1. Shared shapes

- [ ] **Condense swarm/comm**: the `SwarmState` handle pair is still threaded to ~40
  functions and rebuilt as a literal at each request arm, so the request context that
  ends it is the remaining half. `plans/row-model.md` (step 0.4g, the E2/H2 half);
  engine: `internals/swarm.md`.

## 2. God modules

Staged; each lands whole. The designs are in `plans/`, because none of them fits one
screen: `plans/app-shape.md` for the app, `plans/server-shape.md` for the request path.

- [ ] **Re-core `App`** (`crates/kcode-tui/src/tui/app.rs`), the largest single cost in
  the tree. `plans/app-shape.md`.
- [ ] **Split `handle_client`** (`crates/kcode-app-core/src/server/client_lifecycle.rs`).
  `plans/server-shape.md`.
- [ ] **Condense `tool/communicate.rs`**. `plans/server-shape.md`.
- [ ] **Unify the command surface** (four tables, one identity). `plans/app-shape.md`.
- [ ] (decision) **Collapse `AppRuntimeMode::TestHarness`**. `plans/app-shape.md`.
- [ ] (decision) **Re-core the SSH-login state**
  (`crates/kcode-tui/src/tui/app/auth_remote.rs`). `plans/app-shape.md`.

## 3. Spine

- [ ] **Crate spine** (`kcode-base` -> `kcode-app-core` -> `kcode-tui` -> root):
  `pub use kcode_*::*` makes every module path global, so no call site names its
  owning crate. `scripts/check_wildcard_reexport_budget.py` ratchets it (baseline
  13, goal 0). Drive it to zero, or declare the layout cosmetic and collapse it.
  All of §3 is easier once §2 has shrunk the cross-crate surface.

## 4. Tests

- [ ] **The test tree's shape**: real modules instead of `include!`, fewer
  near-duplicate tables, subsystem code out of test files, one home per duplicated
  helper. After §2, because the tree is coupled through `create_test_app`.
  `plans/test-tree.md`.
- [ ] **The known red tests**: `kcode-tui`'s provider-suggestion test and three
  `session_flow` e2e tests fail on a clean tree, undiagnosed, so a red run cannot be
  read until they are explained. `plans/test-tree.md`.

## 5. Hygiene

- [ ] The work list has no user doc. `tasks.jsonl` at the repo root, the `todo`
  tool's three actions, and the close's required result are described only in
  `plans/row-model.md`, which is a plan rather than a manual. It goes in
  `docs/user/` when the list settles, which is late enough to be worth writing
  once: after 0.3, since that is where the row gains its `kind`.
- [ ] Unknown config keys and sections are silently ignored (`toml::from_str` with no
  `deny_unknown_fields`), so a config keeps dead lines with no warning. First real
  instance: `auto_poke_toggle`, which died with the client poke in 0.2.
- [ ] A stale explicit key in `config.toml` silently beats a changed compiled
  default, with no migration or warning.

## Anytime

- [ ] **(decision)** Where does work land: `main`, or a branch with a merge-back?
  Two days and 85 commits went onto `batch-ab-provider-onboarding` while `main` did
  not move and nothing noticed; nothing was lost, but "is `main` green?" could not
  be answered from `main`. Pick one and make it visible: if branches stay, a
  post-commit or session-start line when `git rev-list --count main..HEAD` is
  non-zero. The two hooks in `.githooks/` are graphify's and stay out of it.
- [ ] **(decision)** Does this fork port from upstream? `AGENTS.md` says it "tracks no
  upstream" and that there is "nothing to fetch from `jcode`", but two untracked notes
  at the root describe the opposite: `UPSTREAM-SINCE-0.85.md` and
  `UPSTREAM-PORTABLE.md` cover `v0.85.0 .. master@5f1c091cf` (~398 commits, mirror at
  `~/.kcode/scratch/jcode-upstream.git`) and split what still applies here from what
  this fork deleted. Pick one: delete both notes, give the port-back a plan of its
  own, or make the `AGENTS.md` line name what is actually tracked. The notes call
  themselves scratch, so deleting them is the default.
- [ ] **(decision)** `Action::List` returns every row, not the session's
  (`crates/kcode-app-core/src/tool/todo.rs:64`; the read is
  `load_tasks(dir, session_id)` at `:303`). The engine is already session-scoped on
  `assigned_to` (`bridge.rs:90` in `seed_specs`; `live_turn.rs:259` for ready
  work), so the server's list is the odd one out. Open with C5's client half in
  `plans/row-model.md`: filter for every session, or only for runs? And how does a
  session first see rows it does not hold, so that adopting one is its decision?
  Not a rule: the model does not scope the user's session to its holdings.
- [ ] Not every color derives from a role: `configured_native_color`
  (`kcode-tui-style/src/palette.rs`) attributes a shade to a role only when it
  equals that role's default, so hardcoded `Color::Rgb(...)` shades pass through
  and `/colors` cannot recolor them. The swarm path is covered by
  `plans/row-model.md` D1; what remains is `login_picker.rs`
  `PANEL_BG`/`PANEL_BORDER` and other orphans. Give each shade a role, or mark it
  intentionally fixed.
- [ ] `now_ms` is defined 4x: `app/observe.rs:212`, `app/split_view.rs:295`,
  `app/todos_view.rs:500`, `kcode-base/src/side_panel.rs:557`. The existing clock
  home is `tui::test_harness::now_ms()`, and routing through it changes behavior
  under the test clock, so that is a fix needing its own verification.
- [ ] Hooks are unobservable: no `/hooks`, no listing, no dry-run. A typo looks
  identical to a hook that does nothing.
- [ ] Blocked calls are invisible: `pre_tool` stderr goes to the model, nothing
  tells the user; hook failures are logged and dropped.
- [ ] Reduce the always-on per-request tool cost. Two suite tests that capped tool
  and parameter descriptions (20 / 25 tokens) were removed rather than fixed, so
  nothing guards this; re-measure before acting. Direction: keep only the call
  contract always-on (name, one-line description, terse parameter shape plus
  enums/required) and put the prose in the bundled, version-matched docs
  (`kcode_docs`), echoed in the tool's own error when a call is wrong. Control
  shape: one aggregate schema-token budget instead of per-item caps plus the
  `swarm` exemption. The same gap has a second face: nothing in `kgrep`'s description
  or the system prompt connects "search" to `kgrep`, so a model that has not already
  learned the name does not reach for it.
