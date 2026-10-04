# The loose ends

Work that belongs to no other subject: small, independent, each with its own
measurement. Nothing here gates another lane, and no other lane gates it. The crate
spine is the one structural item; the two calls are the maintainer's.

## Tasks

- [ ] **The provider dispatch fan-out.** `kcode-base/src/provider/mod.rs` (2,858
 lines) re-matches `self.active_provider()` 24 times (28 `ActiveProvider::Claude`
 arms) across the `Provider` impl's 43 methods. One dispatch seam (a slot lookup or
 a macro) removes the repeated 8-arm matches. The arms differ in failover and catalog
 behavior, so verify with the provider suites. (A1, 2026-10-03.)
- [ ] **(decide) The provider tiers.** 61,824 lines in `crates/kcode-provider*` (src)
 plus `kcode-base/src/provider/*` (2,858 in `mod.rs` alone), `provider_catalog.rs` and
 `provider-metadata/catalog.rs`: three homes for one provider's identity and catalog
 (`provider-X` metadata, `provider-X-runtime` transport, the base and metadata tables),
 and `kcode-provider-bedrock` is referenced once from outside its crate. Decide which
 tier owns the catalog, then collapse. This is the repo's largest non-TUI surface and
 it is not the audit's safe tier. (A1.)
- [ ] **The catchup types are in the wrong crate.** `PersistedCatchupState` and
 `CatchupBrief` live in `kcode-task-types` and are used only by
 `kcode-app-core/src/catchup.rs`; move them to the user so the crate named for the
 task concept holds the task concept. (A1.)
- [x] **`tasks.bak` is the store's own backup, and it was tracked.** Landed
 2026-10-03: `storage::write_bytes` keeps the previous list as `<path>.bak` beside
 `tasks.jsonl` for crash recovery, so the file is derived state that reappears on
 every list write (it showed up as a diff during A2), not a stale copy. Untracked
 and ignored. (A1.)
- [ ] **The swarm gallery's render functions are now demo-only.** S2 removed every
 production TUI consumer of `kcode-tui-render/src/swarm_gallery.rs` (the adapter, the
 strip, the page and the dock), so `render_gallery`, the strip/dock/page renderers,
 `GalleryTodo`, `GalleryToolIntent` and `members_to_tiles` are called only by
 `examples/swarm_gallery_live.rs` and the fuzz test. Delete the renderer down to the
 helpers the pinned list uses (`is_active_status`, `status_glyph`, `status_accent`,
 `humanize_age`, the spinner constants), or say the demo is worth the file. (S2.)
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
 cannot recolor them. The swarm path is `plans/task-flow.md` S3; what remains is
 `login_picker.rs` `PANEL_BG`/`PANEL_BORDER` and other orphans. Give each shade a
 role, or mark it intentionally fixed.
- [ ] **`now_ms` is defined 3x**: `app/observe.rs:212`, `app/split_view.rs:295`,
 `kcode-base/src/side_panel.rs:557`. The existing clock home
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
- [ ] **(decision) Does a machine-wide level come back?** 0.5 leaves the level to the
 session, and its default to the model table; config no longer sets one, so a user who
 always wants `high` sets it once per session (`/effort` persists on the session) and a
 repo or CI cannot pin a level at all (`what-was-removed.md` names both losses). The
 default answer is to live with it: the friction is one command per new session, and the
 decision wants evidence that a daily driver hits it. If it comes back, its shape is
 settled: one key with one meaning (a new session's starting level, not a per-family
 provider override), read at resolution time so the session field stays a human's
 choice, and no env var. A `kcode run --effort` flag is a separate, additive call, and
 only if scripts need a level without touching config.
- [ ] **(decision) Does this fork port from upstream?** `AGENTS.md` says it "tracks no
 upstream" and that there is "nothing to fetch from `jcode`", but two untracked notes
 at the root describe the opposite: `UPSTREAM-SINCE-0.85.md` and
 `UPSTREAM-PORTABLE.md` cover `v0.85.0 .. master@5f1c091cf` (~398 commits, mirror at
 `~/.kcode/scratch/jcode-upstream.git`) and split what still applies here from what
 this fork deleted. Pick one: delete both notes, give the port-back a plan of its
 own, or make the `AGENTS.md` line name what is actually tracked. The notes call
 themselves scratch, so deleting them is the default.
- [ ] **The session context block prints the version hash twice, and its fat half is
 unproven.** `build_session_context` (`kcode-base/src/prompt.rs:482`) is added once per
 session (`session.rs:879`) and carried on every request after that. Measured on a
 worker's transcript (2026-10-04): 516 chars, about 129 tokens, split date/time/zone ~90,
 OS/arch/version ~90, Hardware ~230 (45 percent; the GPU line alone is 92), cwd and git
 ~110. The certain defect: the version line reads `v0.1.0-dev (cd3656e7) (cd3656e7)`,
 because `version()` already formats `v{semver} ({hash})` (`kcode-build-meta/src/lib.rs:64`)
 and `prompt.rs:488` wraps it again, so one line drops the second parenthetical. The open
 question is whether the Hardware block ever steers a decision, which is a read of real
 transcripts rather than a benchmark. The cost is already reported per prompt layer
 (`prompt.rs:220`).
- [ ] **`remaining_member_count` counts the departing member.** `server/swarm.rs:992`
 reports `swarm_session_ids(...).len()` while the leaving member is still in the map (the
 removal is the caller's, and some callers take it out first), so the field over-reports
 by one: the first fan-out run logged 3 after the first of three members left and 2 after
 the second, which reads as a leak when nothing leaked (2026-10-04). Name it for what it
 is, or take the count after the removal.
