# The loose ends

Work that belongs to no other subject: small, independent, each with its own
measurement. Nothing here gates another lane, and no other lane gates it. The crate
spine is the one structural item; the two calls are the maintainer's.

## Tasks

- [ ] **The crate spine.** `kcode-base` -> `kcode-app-core` -> `kcode-tui` -> root,
 with `pub use kcode_*::*` making every module path global, so no call site names its
 owning crate. `scripts/check_wildcard_reexport_budget.py` ratchets it (baseline 13,
 goal 0). Drive it to zero, or declare the layout cosmetic and collapse it. Easier
 once `plans/app-shape.md` and `plans/server-shape.md` have shrunk the cross-crate
 surface.
- [ ] **Unknown config keys and sections are silently ignored** (`toml::from_str`
 with no `deny_unknown_fields`), so a config keeps dead lines with no warning. First
 real instance: `auto_poke_toggle`, which died with the client poke in 0.2.
- [ ] **A stale explicit key in `config.toml` silently beats a changed compiled
 default**, with no migration or warning.
- [ ] **Hook observability.** The gaps as a user meets them are in `docs/user/hooks.md`
 under "Not implemented yet"; the work is a `/hooks` command, a listing of the
 configured hooks, and a dry-run, so a typo in a command string stops looking
 identical to a hook that runs and does nothing.
- [ ] **Blocked calls and hook failures reach nobody.** Add a per-session count of
 `pre_tool` blocks and of observer failures, surfaced to the user; today the first goes
 only to the model and the second only to the log.
- [ ] **Not every color derives from a role**: `configured_native_color`
 (`kcode-tui-style/src/palette.rs`) attributes a shade to a role only when it equals
 that role's default, so hardcoded `Color::Rgb(...)` shades pass through and `/colors`
 cannot recolor them. The swarm path is `plans/row-model.md` D1; what remains is
 `login_picker.rs` `PANEL_BG`/`PANEL_BORDER` and other orphans. Give each shade a
 role, or mark it intentionally fixed.
- [ ] **`now_ms` is defined 4x**: `app/observe.rs:212`, `app/split_view.rs:295`,
 `app/todos_view.rs:500`, `kcode-base/src/side_panel.rs:557`. The existing clock home
 is `tui::test_harness::now_ms()`, and routing through it changes behavior under the
 test clock, so that is a fix needing its own verification.
- [ ] **Reduce the always-on per-request tool cost.** Two suite tests that capped tool
 and parameter descriptions (20 / 25 tokens) were removed rather than fixed, so
 nothing guards this; re-measure before acting. Direction: keep only the call
 contract always-on (name, one-line description, terse parameter shape plus
 enums/required) and put the prose in the bundled, version-matched docs
 (`kcode_docs`), echoed in the tool's own error when a call is wrong. Control shape:
 one aggregate schema-token budget instead of per-item caps plus the `swarm`
 exemption. The same gap has a second face: nothing in `kgrep`'s description or the
 system prompt connects "search" to `kgrep`, so a model that has not already learned
 the name does not reach for it.
- [ ] **(decision) Where does work land**: `main`, or a branch with a merge-back? Two
 days and 85 commits went onto `batch-ab-provider-onboarding` while `main` did not
 move and nothing noticed; nothing was lost, but "is `main` green?" could not be
 answered from `main`. Pick one and make it visible: if branches stay, a post-commit
 or session-start line when `git rev-list --count main..HEAD` is non-zero. The two
 hooks in `.githooks/` are graphify's and stay out of it.
- [ ] **(decision) Does this fork port from upstream?** `AGENTS.md` says it "tracks no
 upstream" and that there is "nothing to fetch from `jcode`", but two untracked notes
 at the root describe the opposite: `UPSTREAM-SINCE-0.85.md` and
 `UPSTREAM-PORTABLE.md` cover `v0.85.0 .. master@5f1c091cf` (~398 commits, mirror at
 `~/.kcode/scratch/jcode-upstream.git`) and split what still applies here from what
 this fork deleted. Pick one: delete both notes, give the port-back a plan of its
 own, or make the `AGENTS.md` line name what is actually tracked. The notes call
 themselves scratch, so deleting them is the default.
