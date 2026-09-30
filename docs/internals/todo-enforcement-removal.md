# Todo enforcement removal

Status: step 1 landed (2026-09-30), step 2 next, step 3 now unblocked. The six
decisions are resolved below.

## Intent

The todo list is a display of the plan. The client must not enforce it.

- **Weak self-assessment must not drive the model onward.** Today the ladder
  does exactly that: on a weak score it queues a hidden continuation that says
  "do more validation ... do not reply or wait for the user". The wanted
  behavior is that the model may stop and ask, and that a weak assessment is
  visible to the user. Deleting the ladder serves this; keeping it fights it.
- **The poke is opt-in.** `features.auto_poke` defaults to false
  (`kconfig-types/src/lib.rs:952`) and the ladder is initialised from it
  (`tui_lifecycle.rs:428`). Nothing here changes the default experience.
- **Keep:** the `todo` tool, its fields, `/poke`, Ctrl+P, the flag, and the
  guardrail breaker. Capability is fixed; only the enforcement goes.

## End state (implemented in step 1)

```
turn ends
  ├─ guardrail-refused turns > 2?   → stop, tell the user   (kept)
  ├─ auto-poke armed?                no  → nothing
  ├─ any todo open?                  no  → nothing
  ├─ a follow-up already queued?     yes → nothing
  ├─ list unchanged since last poke? yes → nothing
  └─ queue the one message:
     "You have N incomplete todo(s). Continue working, or update the todo tool."
```

No digest, no ownership or confidence check, no budget, no final-response
handoff. Six injected messages are one, and none of them judges the work.

## Step 1 landed (2026-09-30)

Enforcement deleted, schema untouched, display untouched. `App` loses six
fields, `kcode-base/src/todo.rs` drops from 2085 lines to 576 and out of the
size budget, `app-core/tool/todo.rs` 2274 -> 1715, `src/cli/commands.rs`
1515 -> 1281, and the TUI ladder's 248 lines become a 57-line function.

Kept deliberately, and why:

- The pure predicates the todo card still calls (`feedback_loop_*_passes`,
  `intent_understanding_passes`, `required_*`). Deleting them now would force
  the card to be rewritten in the same landing; step 3 removes the fields they
  read and the predicates with them.
- The message constants and the text classifier. `is_auto_poke_message` lists
  every spelling so a transcript resumed from an older session renders those
  continuations as system notices rather than as the user's own prompts. Step 2
  replaces the matching and deletes the group.
- `AUTO_POKE_DECISION action=queue_continuation`, so the probe's poke count
  works on both sides.

Corrections to this doc's original inventory, found while making the change:

- Headless has **three** poke loops, not two: `plain`, `capture` and `ndjson`.
  The last also emitted gate-shaped JSON events; it now emits `auto_poke` and
  `auto_poke_stopped` only. `scripts/kcode_harbor_agent.py`, the only in-repo
  consumer of that stream, reads only `type == "done"`.
- Step 3 also edits `crates/kcode-task-types/src/lib.rs`, where the todo types
  actually live, plus the re-export at `kcode-base/src/todo.rs:29`.
- `tui/ui_messages.rs` renders the goal scores through the same pass predicates
  the gate used, so the card is coupled to the gate.
- The enumerated test files could not all be deleted whole.
  `tui/app/tests/support/part_01.rs` holds helpers every sibling include needs,
  and `ui_messages/tests.rs` and `session_tests/cases.rs` still compile and
  still cover the kept display. What went is the tests that referenced removed
  symbols, per file.

Runtime check, one isolated `kcode run` with auto-poke armed on a task that
leaves two todos open: exactly one poke was sent, no second poke followed on the
unchanged list, and the stored session contains none of the removed messages.
`kcode-tui --lib` is 1830 passed / 1 failed, and that failure,
`test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`,
fails identically at `4ef8c3e1` without this change.

## Remaining work

### Step 2: replace the text classifier with a marker

`is_auto_poke_message` and `auto_poke_display_summary` decide whether a queued
message is ours by matching its text, which is why every reword added a
constant. The one poke that survives is still persisted as a `Role::User` turn,
so the recognition still matters: without it a reload, resume or remote attach
renders the poke as the user's own prompt.

Give the outbound queue item a kind, set when the poke is enqueued, and move
these four call sites onto it:

- `kcode-base/src/session/render.rs` (restored-history rendering)
- `kcode-tui/src/tui/app/overnight_card.rs` (the live card path)
- `kcode-tui/src/tui/app/commands.rs::is_poke_message`
- `kcode-tui/src/tui/app/commands.rs::queued_messages_are_only_pokes`, which
  `remote.rs` uses to preserve the visible turn

Then delete the classifier, `auto_poke_display_summary`, and every
`PRE_*`/`LEGACY_*`/`LABELED_*` constant together with the retention note above
them in `kcode-base/src/todo.rs`. This is where the fresh tests for the poke and
the marker belong, because step 1 deleted the tests that covered the removed
behavior.

### Step 3: shrink the `todo` schema to a display

`id`/`content`/`status`/`priority`/`group`, dropping `plan`, `goals`,
`confidence` and `completion_confidence`: 1,086 always-on tokens to roughly 120.

Surfaces:

- `crates/kcode-task-types/src/lib.rs`: `TodoGoal`, `TodoPlan` and their four
  change types.
- `kcode-base/src/todo.rs`: the pass predicates, the goals and plan storage, the
  type re-export, and the score history helpers.
- `tui/ui_messages.rs`: the score rendering and the predicates it calls.
- The plan and goal threading in `todos_view.rs`, `helpers.rs`, the desktop card
  payload, and `src/cli/commands.rs`.
- The tool description gains decision 4's line; `internals/todo-calibration.md`
  and its `kcode-docs` path go with the fields.

## Validation

The before/after probe recipe for comparing verification and poke count across
the rework lives in `dev/todo-rework-ab-probe.md`; this doc does not duplicate
it.

## Decisions resolved (2026-09-30)

1. **Drop it.** The recorder goes, and so does the nudge. Its own text said
   "avoid asking the user", which is the opposite of this change's intent, and
   every other write-time check was already deferred to turn end for the same
   reason. `understands_user_intent_history` loses its only reader with it, so
   the tool no longer appends to that vector.
2. **Drop the rubric.** Nothing compares the request to the list: the model
   scores its own understanding, and the same misunderstanding that drops a
   requirement also rates the understanding as clear, so the check cannot catch
   the failure it is named after. Its only other reader is the todo card.
3. **Drop both scores** with the rubric at step 3. `completion_confidence` and
   `confidence` keep no reader once the evaluator is gone, and the widget shows
   `status`.
4. **One line in the tool description**: *name the check that proves it is done
   and report its actual result.* That is the whole surviving value of the
   rubric, and it costs no fields.
5. **Keep one plain loop.** The three loops in `src/cli/commands.rs` become one
   shared decision function; the gates inside them go.
6. **Nothing to surface.** With no score to report there is no weak assessment
   to show. If a notice is wanted later it belongs in the model's own message,
   not in a harness notice about a score the user cannot see.

Two more decisions taken during the pass, both capacity cuts:

- **`plan.user_intention` goes.** Its readers were the gate and the card. The
  transcript already holds the user's request verbatim, so a stored restatement
  is a second copy to keep true. If the list proves hard to check against the
  request, render the user's own last message at the top of the todo view
  instead of storing a model-written one.
- **`auto_poke_default_on` goes.** It existed only to distinguish "armed by the
  config default" from "armed by `/poke on`" across the cycle boundary. With one
  poke there is no cycle, so armed is one bool, and `last_auto_poke_fingerprint`
  is the loop guard.
