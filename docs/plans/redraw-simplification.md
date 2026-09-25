# Redraw and perf simplification

Status: proposal
Problem: four overlapping mechanisms decide when the TUI paints. Adding one
time-dependent element means editing all four, and every new element multiplies
the matrix.

## The problem, measured

What exists today, and what a single new time-dependent element must touch:

| mechanism | code | cost per new element |
|---|---|---|
| cadence chain | `redraw_interval_with_policy_and_animation` (10 branches), `redraw_interval_with_policy`, `redraw_interval` | 1-2 new branches, ordered correctly |
| draw gate | `periodic_redraw_required`, `_excluding_idle_animation`, `_inner`, `live_activity_redraw_reason` + the reason table | 1-2 new terms in a ten-term chain, plus a reason name |
| tier matrix | `PerformanceTier` (3) x 9 policy fields x auto-score inputs (load, memory, SSH, WSL, terminal) | new arms in every `match`, new synthetic profile |
| second draw path | animation-only partial repaint, `idle_animation_only_available` | must decide whether the element is eligible |

So roughly five files and a test-name table per widget. The named-reason table
exists only to debug that chain, which is the signal: if the policy needs names
to be readable, it has too many predicates.

## Target

One rule. A state that changes without input registers a `Live` element. The
scheduler takes the minimum interval over the live set and draws iff the set is
non-empty.

```rust
enum Live {
    Streaming,            // tokens arriving
    Spinner,              // swarm / picker glyphs, 12.5fps off the wall clock
    Countdown(Instant),   // overscroll, rate-limit reset, cache-cold window
    Notice(Instant),      // toasts retiring
    Decoration,           // only if the idle donut survives phase 2
}

impl Live {
    fn interval(self, now: Instant) -> Duration { /* what this element needs */ }
}

// the whole policy:
let tick = live.iter().map(|l| l.interval(now)).min().unwrap_or(IDLE);
let draw = !live.is_empty();
```

Every element owns its own cadence where the element is defined, which is also
where the person adding it is already looking. Adding one is one arm and one
registration.

## Phases

Each phase ships on its own and is verifiable alone. Deletion-first throughout.

1. **Live set, no behavior change.** Move each existing predicate verbatim into
   a `Live` variant whose `interval()` returns exactly today's cadence. Replace
   the cadence chain in `redraw_interval*`. Delete `live_activity_redraw_reason`
   and the duplicate `periodic_redraw_required*` pair, keeping one gate that is
   just `!live.is_empty()`.
   Done when the existing redraw-cadence tests pass unchanged and `draw-stats`
   reports the same intervals per state.

2. **Decide the donut.** It is now the only animation, and it carries a 930-line
   `jcode-tui-anim` crate used by nothing else, an `opt-level = 3` override in
   every profile, `disabled_animations` by name, and the partial-repaint path.
   Default: delete the scene and fall back to a two-frame glyph cycle on the
   spare `SPINNER_FRAMES` (already in `jcode-tui-style`). Alternative: keep it as
   a single `Live::Decoration` at a fixed low rate.
   Done when either `Live::Decoration` does not exist, or it is the only place
   the animation cadence is mentioned.

3. **Tiers out.** `PerformanceTier` + auto-scoring exists to cap FPS and gate
   one decoration, and it makes behavior machine-dependent and untestable except
   as a matrix. Replace with plain policy fields: `fps`, `idle_fps`. Keep the
   real compatibility decisions, just not as a tier: WSL/Windows Terminal
   focus/keyboard tweaks, and the macOS glyph-safe redraw cap. `display.performance`
   and `JCODE_PERF_TIER` go; the loader already ignores unknown keys.
   Done when `perf.rs` has no tier enum and no load/memory scoring.

4. **One draw path.** Delete the animation-only partial repaint and
   `idle_animation_only_available`, keeping one path. This is gated on a
   measurement: the point of the second path was that a full frame re-renders
   the transcript, and the body cache is the real fix. Keep the fast path only if
   `draw-stats` frame time on an idle animated screen is materially worse
   without it.
   Done when there is one draw site or a recorded reason the second one stays.

## Order and risk

Phases 1 and 3 are independent. Phase 2 gates 4 (no decoration, no partial
path). Phase 1 is the only one touching the render loop, and it is a pure
refactor with the existing cadence tests as the net.

## What is deliberately kept

- The `tokio::select!` loop over input / spinner / timer / bus. It blocks
  instead of spinning, and that part is right.
- `fps` and `idle_fps` as config, and a single env override. Real terminals need
  tuning, and a knob is cheaper than a tier.
- The body cache. It is what makes "just draw a frame" affordable.
- The deep-idle crawl, just as one element's interval rather than a predicate
  every branch repeats.

## Success criteria

- Adding a time-dependent element touches one file, one enum arm, one test.
- `redraw_interval` is `min` over the live set. No predicate chain, no named
  reason table.
- No `PerformanceTier`.
- One draw path.
- `jcode-tui-anim` deleted or justified by more than decoration.

## Non-goals

- **Not** touching input handling or the select loop's sources.
- **Not** adding configuration. Phase 3 removes a key; nothing is added.
- **Not** revisiting color. That is `limited-palette.md` and is already done.
- **Not** a rewrite. Every phase is a deletion or a move, on top of current
  behavior, with existing tests as the acceptance net.
