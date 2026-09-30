# Todo enforcement removal

Status: phase 1 (steps 1-3) landed 2026-09-30. Phase 2 (stages 4-5) is recorded
below, not yet planned. The six decisions are resolved below.

## Intent

The todo list is a display of the plan. The client must not enforce it.

- **Weak self-assessment must not drive the model onward.** The deleted ladder
  did exactly that: on a weak score it queued a hidden continuation that said
  "do more validation ... do not reply or wait for the user". The wanted
  behavior is that the model may stop and ask, and that a weak assessment is
  visible to the user. Deleting the ladder served this; keeping it fought it.
- **The poke is opt-in.** `features.auto_poke` defaults to false
  (`kconfig-types/src/lib.rs:952`) and the ladder was initialised from it
  (`tui_lifecycle.rs:428`). Nothing here changed the default experience.
- **Kept:** the `todo` tool, its fields, `/poke`, Ctrl+P, the flag, and the
  guardrail breaker. Capability is fixed; only the enforcement went.

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
  the card to be rewritten in the same landing; step 3 removed the fields they
  read and the predicates with them.
- The message constants and the text classifier. `is_auto_poke_message` listed
  every spelling so a transcript resumed from an older session rendered those
  continuations as system notices rather than as the user's own prompts. Step 2
  replaced the matching with the queue's `[SYSTEM: ...]` shape and deleted the
  group.
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

## Step 2 landed (2026-09-30)

The poke is queued wrapped as `[SYSTEM: ...]`, the outbound queue's existing
convention. `partition_queued_messages` turns that into a reminder-only turn, so
the model reads it as a system reminder with no user content, and the stored
message is empty, which the history renderer already skips.

Deleted: `is_auto_poke_message`, `auto_poke_display_summary`, the 22 message
constants and the retention note (`kcode-base/src/todo.rs` 576 -> 249), the
classifier's four call sites (three in `session/render.rs`, one in
`render/response_stats.rs`, plus `overnight_card.rs`), `note_todo_gate_result`
and `todo_gate_notice` with their caller (`observe.rs` 284 -> 217),
`queued_messages_are_only_pokes`, the two poke branches in the dispatch, and the
third return value of `partition_queued_messages`. A queued system message is no
longer displayed at all, because whoever queues it shows its own notice.

Kept and re-homed: `queued_system_message` beside its parser in `helpers.rs`,
and `queue_poke_message` beside the other poke helpers in `commands.rs`.

Cost, deliberate: a session persisted before this change holds tier messages as
plain user-role content, and resumed history now renders those as if the user
wrote them. Only pre-change data is affected.

Unverified by static checks, and the one thing to confirm on a live build: the
poke's channel changed from user content to a system reminder. One isolated
`kcode run` with auto-poke armed, on a task that leaves items open, must show
the model continuing rather than only replying. If it chats instead, the
fallback is to queue the poke as user content with exactly one text constant.

## Step 3 landed (2026-09-30): the `todo` schema is a display

Five fields - `id`, `content`, `status`, `priority`, optional `group` - with the
first four required, and decision 4's line in the description. 1,086 always-on
tokens to roughly 120. `plan`, `goals`, `confidence` and `completion_confidence`
are gone from the tool, the card, the widget, the todos view, the state snapshot,
the turn notification, the improve/refactor status reports and the todo-change
list; `TodoGoal`/`TodoPlan` and the ten assessment enums they used are deleted, so
`TodoItem` keeps the display fields plus the two the swarm plan projection needs
(`blocked_by`, `assigned_to`). A write that still carries the retired fields is
ignored rather than rejected, and `internals/todo-calibration.md` is deleted with
them; the surviving idea is the description's one line, name the check that proves
an item done and report its result.

Two process findings from this step, both worth keeping:

- **The gate compiles test code (`--all-targets`) but never runs it**, so a
  test-only assertion breakage passes the gate. Run the suite itself before
  calling a landing done. Two failures here were introduced in earlier steps and
  stayed invisible through the gate.
- **Beware of deleting by block scan.** Two of the edits here silently removed
  one line past the intended block (an `impl` opener, then a `fn` signature),
  because a scan that ends at the first column-zero `}` starts counting before
  the body opens when the signature spans lines. Both were caught by a compile,
  neither by review. Prefer explicit text anchors; verify with a build.

## Validation

The before/after probe recipe for comparing verification and poke count across
the rework lives in `dev/todo-rework-ab-probe.md`; this doc does not duplicate
it.

## Superseded (2026-10-01)

The forward-looking half of this doc - the desired final state, the constraints
settled with it, and the phase split - is superseded by `plans/work-list.md`,
which widens the target from the todo feature to the work list the swarm also
serves. The landed record above and the decisions below are kept: they are why
the enforcement tier must not come back.

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
