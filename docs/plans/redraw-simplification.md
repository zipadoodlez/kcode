# Redraw and perf simplification

Status: proposal
Problem: four mechanisms decide cadence and painting, and one of them (the tick)
is also the app's housekeeping pump. Adding one time-dependent element means
editing all four.

## The problem, measured

`handle_tick` (`app/local.rs`) runs ~20 jobs: drains `stream_buffer`, refreshes
todos/pinned/side-panel, polls the model and session pickers, prunes background
tasks, expires notices, progresses the overscroll countdown. That is the app's
clock, not a repaint scheduler, and it must run even when nothing is on screen.
So the tick stays. What does *not* deserve to exist is everything layered on top
of it:

| mechanism | code | per new element |
|---|---|---|
| cadence chain | `redraw_interval_with_policy_and_animation` (10 branches), `redraw_interval_with_policy`, `redraw_interval` | 1-2 new branches, correctly ordered |
| draw gate | `periodic_redraw_required`, `_excluding_idle_animation`, `_inner`, `live_activity_redraw_reason` + named-reason table | 1-2 new terms in a ten-term chain, plus a reason name |
| tier matrix | `PerformanceTier` (3) x 9 policy fields x auto-score inputs (load, memory, SSH, WSL, terminal) | new arms in every `match`, a new synthetic profile |
| second timer + second draw path | `status_spinner_interval` select arm, animation-only partial repaint, `idle_animation_only_available` | decide eligibility for both |

The named-reason table exists only so the chain can be debugged. That is the
signal: if the policy needs names to be readable, it has too many predicates.

## Target

The floor: **one timer, one bool, two constants, one draw path.**

```rust
// The only cadence question. True when something visible is mid-motion or
// mid-arrival: processing/streaming, a spinner, a live countdown, a retiring
// notice, a held drag. Everything else is served by events and housekeeping.
fn wants_fast_tick(&self) -> bool;

loop {
    let period = if self.wants_fast_tick() { FAST } else { SLOW };
    // one timer; sleep on input/bus events otherwise
    on tick:  housekeeping(); if dirty { draw() }
    on event: apply();        if dirty { draw() }
}
```

No `Live` enum, no `min(interval)` over per-element cadences, no reason table.
A per-element cadence is only worth adding when one specific element visibly
stutters, and at that point the fix is one constant next to that element, not a
scheduler feature.

The drag case shows the shape: instead of a dedicated 60ms tick, let a fixed
tick advance a variable number of lines. Speed then follows the gesture by
construction rather than by a special-case interval.

## Phases

Two, not four. The first buys most of the second.

1. **Delete the decoration and its two escape hatches.** The idle donut is now
   the only animation; it is what `animation_fps`, the partial-repaint path, the
   spinner-only select arm, and name-based `disabled_animations` exist to serve.
   Delete it (a two-glyph cycle on the existing `SPINNER_FRAMES` if idle motion
   is wanted), then delete the partial repaint, the second timer, `jcode-tui-anim`
   (930 lines, used by nothing else), and the `opt-level = 3` overrides.
   One product decision, everything else falls out.
   Done when there is one timer and one draw site.

2. **Collapse cadence to the rule above.** Replace both chains with
   `wants_fast_tick()` plus `FAST`/`SLOW`, keeping the tick's housekeeping intact.
   Then delete `PerformanceTier` and its auto-scoring (replace with the two
   constants and the existing `fps` knob), keeping only the real compatibility
   fixes: WSL/Windows Terminal focus and keyboard protocols, and the macOS
   glyph-safe redraw cap. `display.performance` and `JCODE_PERF_TIER` go; the
   loader already ignores unknown keys.
   Done when `redraw_interval` is gone, `perf.rs` has no tier enum, and the
   housekeeping tick still runs.

Phase 1 is a deletion with no behavior to preserve beyond the donut itself.
Phase 2 changes cadence, so it lands after 1 with the existing cadence tests as
the net.

## What is deliberately kept

- The tick. It is the housekeeping pump (`stream_buffer` is drained there), and
  `handle_tick`'s twenty jobs are a separate, larger cleanup than this plan.
- The `tokio::select!` loop. It blocks on input/bus/timer instead of spinning.
- `fps` as config and one env override. Real terminals need tuning; that is the
  knob a minimal model cannot replace.
- The body cache, which is what makes "just draw a frame" affordable.

## Success criteria

- One timer, one draw path.
- `wants_fast_tick()` is the only cadence question. No `redraw_interval*`, no
  named reasons, no deep-idle predicate repeated in every branch.
- No `PerformanceTier`.
- `jcode-tui-anim` deleted.
- Adding a time-dependent element touches the element and at most one arm of
  `wants_fast_tick`.

## Non-goals

- **Not** making the tick event-driven. Draining `stream_buffer` on the tick is
  load-bearing; moving it to a wake signal is a separate change with its own
  risk, only worth it if the idle tick shows up in a profile.
- **Not** rewriting `handle_tick`'s twenty jobs.
- **Not** adding configuration. Phase 2 removes a key.
- **Not** touching input handling, or color (`limited-palette.md`).
