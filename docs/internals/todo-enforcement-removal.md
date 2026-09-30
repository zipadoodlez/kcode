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

## Steps

1. Delete the enforcement tier, keep one poke. The TUI ladder and the headless
   copy, every continuation they queue, the digest, the observation store, the
   review cycle, the ownership/confidence/spike checks, the five gate-only `App`
   fields, and `record_reframe_observations`.
2. In the same change, delete the synthetic-text classifier.
3. Blocked on the decisions: shrink the `todo` schema to a display
   (`id`/`content`/`status`/`priority`/`group`), from 1,086 always-on tokens to
   roughly 150.

## End state

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
handoff. Six injected messages become one.

## The tier is duplicated

Removing only the TUI copy leaves the behavior in headless runs.

- TUI: `crates/kcode-tui/src/tui/app/input.rs`.
- Headless `kcode run`: `src/cli/commands.rs`, roughly 340 lines.

Both go in the same change.

## Touch inventory

### `crates/kcode-base/src/todo.rs` (2085, ratcheted): the home

Delete: `GateObservationKind`, `GateObservation`, `append_gate_observations`,
`load_gate_observations`, `clear_gate_observations`, `TODO_GATE_DIGEST_PREFIX`
(+ `PRE_COMPACT`, `LABELED`), `build_gate_digest`,
`observation_score_later_cleared`; `TODO_LONG_SESSION_REVIEW_MESSAGE` (+
`PRE_*`), `TodoReviewState`, `todo_review_path`, `update_todo_review_cycle`,
`take_long_session_review_if_due`, `TODO_LONG_SESSION_REVIEW_AFTER`; `build_todo_ownership_continuation_message`,
`build_todo_completion_continuation_message`,
`build_todo_confidence_spike_continuation_message` with their `PRE_*`/`LEGACY_*`
constants; `TODO_CONFIDENCE_SPIKE_LEVELS`, `spike_completed_todos`;
`TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE` (decision 1);
`completed_groups_have_sufficient_delivery` and the delivery helpers if nothing
else reads them; `is_auto_poke_message`, `auto_poke_display_summary`, and every
`PRE_*`/`LEGACY_*`/`LABELED_*` constant.

Keep: `build_auto_poke_message`, `TodoItem`/`TodoGoal`/`TodoPlan` plus their
load/save, the confidence types (decision 3), and the canonical status helpers.

### `crates/kcode-tui/src/tui/app/input.rs`

Delete the ladder body of `schedule_auto_poke_followup_if_needed` and
`deliver_deferred_gate_digest_if_needed`. Keep `schedule_turn_end_followups`,
the guardrail breaker, and the one poke.

### `crates/kcode-tui/src/tui/app/commands.rs`

Delete `TodoConfidenceSummary`, `todo_confidence_summary`,
`build_todo_confidence_summary_message`, `format_todo_completion_confidence`,
and the `is_auto_poke_message` use.

### `crates/kcode-tui/src/tui/app.rs`

Delete the five gate-only fields: `todo_confidence_spike_challenged`,
`todo_gate_digest_delivered`, `todo_completion_gate_attempts`,
`last_todo_ownership_fingerprint`, `todo_final_response_requested`.

### Classifier users (step 2)

`crates/kcode-tui/src/tui/app/overnight_card.rs`,
`crates/kcode-base/src/session/render.rs`.

### `crates/kcode-app-core/src/tool/todo.rs` (2274, ratcheted)

Delete `record_reframe_observations` and its gate imports, the
`append_gate_observations` call, and the `update_todo_review_cycle` call.

### `src/cli/commands.rs` (1515, ratcheted)

Delete `run_command_auto_poke_enabled`, `run_command_auto_poke_max_turns`,
`run_command_auto_poke_limit_reached`, `take_run_gate_digest`,
`take_run_gate_digest_if_turn_ended`, `build_run_auto_poke_follow_up_from_todos`,
`build_run_todo_validation_message`, and the two `*_with_auto_poke` loops. If
headless poking is still wanted, reduce them to a plain poke loop with no gates
(decision 5).

### Keep, small edits (flag and plumbing only)

`kcode-config-types/src/lib.rs`; `kcode-base/src/config/{default_file,
display_summary,env_overrides}.rs`; `kcode-tui/src/tui/keybind.rs`,
`tui/app/input_help.rs`, `tui/app/hotkey_feedback.rs`,
`tui/app/commands_overnight.rs`, `tui/app/remote*`, `tui/app/turn_notify.rs`,
`tui/app/model_context.rs`, `tui/app/observe.rs`, `tui/app/tui_lifecycle.rs`.

### Tests deleted with the tier, not fixed

`src/cli/commands_tests.rs`; `tui/app/tests/state_model_poke_03.rs`,
`tui/app/tests/remote_events_reload_01/02/04/05`,
`tui/app/tests/remote_startup_input_01/02`,
`tui/app/tests/scroll_copy_01/part_02`, `tui/app/tests/support/part_01.rs`;
`tui/app/commands_tests.rs`; `tui/ui_messages/tests.rs`;
`kcode-base/src/session_tests/cases.rs`; inline `#[cfg(test)]` modules in
`kcode-base/src/todo.rs`, `tui/app/input.rs`, `app-core/tool/todo.rs`, and
`config_tests.rs`.

### Ratchet

Three ratcheted files shrink: `kcode-base/src/todo.rs` (2085),
`kcode-app-core/src/tool/todo.rs` (2274), and `src/cli/commands.rs` (1515). A
ratchet fails on an unrecorded improvement, so the pass ends with
`scripts/check_guardrails.sh --fix` to rebaseline. That is the "intentional
cleanup" the policy names.

## Validation

The result is judged against behavior, not vibes, and the tier is opt-in, so a
before/after probe is cheap. Run one live probe per provider on a task that
forces both a search and a verification, and record whether the model finished
the work and reported a concrete check. If the probe shows the model stops
verifying, the fallback is one line in the tool description or the system
prompt: *"For each goal, name the check that proves it is done and report its
actual result."* That is a replace-the-nudge option, not part of the plan until
the probe asks for it.

## Decisions open

1. The severe-intent write-time interrupt: step 1 deletes it with the recorder.
   Keep it as one exception, or let it go?
2. The eight-dimension goal rubric: keep as model guidance, or drop?
3. `completion_confidence`: after step 1 its only reader is the widget label.
   Collapse it onto `confidence`?
4. A when-to-write trigger for the model: nothing, or one line in the tool
   description?
5. Headless `kcode run`: keep a plain poke loop with no gates, or drop
   auto-poke there too?
6. A weak self-assessment: surface it to the user as a visible notice, or say
   nothing at all?

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

## Step 1 landed (2026-09-30)

Enforcement deleted, schema untouched, display untouched. `App` loses six
fields, `kcode-base/src/todo.rs` drops from 2085 lines to 576 and out of the
size budget, `app-core/tool/todo.rs` 2274 -> 1715, `src/cli/commands.rs`
1515 -> 1281, and the TUI ladder's 248 lines become a 57-line function.

What step 1 kept, deliberately:

- The pure predicates the todo card still calls (`feedback_loop_*_passes`,
  `intent_understanding_passes`, `required_*`). Deleting them now would force
  the card to be rewritten in the same landing; step 3 removes the fields they
  read and the predicates with them.
- The message constants and the text classifier. `is_auto_poke_message` lists
  every spelling so a transcript resumed from an older session renders those
  continuations as system notices rather than as the user's own prompts. Step 2
  replaces the matching with a marker on the queued item and deletes the group.
- `AUTO_POKE_DECISION action=queue_continuation`, so the A/B count metric in
  `dev/todo-rework-ab-probe.md` works on both sides.

Corrections to this doc's touch inventory, found while making the change:

- Headless has **three** poke loops, not two: `plain`, `capture`, and
  `ndjson`. The last one also emitted gate-shaped JSON events; it now emits
  `auto_poke` and `auto_poke_stopped` only. `scripts/kcode_harbor_agent.py`, the
  only in-repo consumer of that stream, reads only `type == "done"`.
- Step 3 also edits `crates/kcode-task-types/src/lib.rs` (the `Todo` types and
  their re-export at `kcode-base/src/todo.rs:29`), which the inventory omitted.
- `tui/ui_messages.rs` renders the goal scores through the same pass predicates
  the gate used, so it is coupled to the gate and is a step-3 surface.
- The enumerated test files could not all be deleted whole: `support/part_01.rs`
  holds helpers every sibling test includes, and `ui_messages/tests.rs` and
  `session_tests/cases.rs` still compile and still cover the kept display. What
  was deleted is the tests that referenced removed symbols, per file.

Runtime check, one isolated `kcode run` with auto-poke armed on a task that
leaves two todos open: exactly one poke was sent, no second poke followed on the
unchanged list, and the stored session contains none of the removed messages.
The TUI suite is 1830 passed / 1 failed, the failure being
`test_remote_fallback_provider_suggestions_normalize_bare_openai_openrouter_routes`,
which fails identically at `4ef8c3e1` without this change.
