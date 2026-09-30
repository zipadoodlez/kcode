# Todo enforcement removal

Status: steps 1 and 2 landed (2026-09-30), step 3 next. The six decisions are
resolved below.

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

## Remaining work

### Step 3: shrink the `todo` schema to a display

`id`/`content`/`status`/`priority`/`group`, dropping `plan`, `goals`,
`confidence` and `completion_confidence`: 1,086 always-on tokens to roughly 120.

**Landed so far (2026-09-30, committed `59c40abb` plus an uncommitted display
pass):** the tool advertises and writes five fields (`id`, `content`, `status`,
`priority`, optional `group`), with `id`/`content`/`status`/`priority` required,
and its description carries decision 4's line. Its merge machinery for goals,
plan and confidence histories is deleted (1708 -> 352 lines), and a write that
still carries the retired fields is ignored rather than rejected. The card
carries tasks only (3864 -> 3038), and the widget, the todos view, the state
snapshot, the turn notification and the todo-change list no longer read goals,
plan or confidence. Remaining work in this step:

- **Six test expectations and fixtures are red** in `kcode-tui`; each asserts
  behaviour this step removed. They are the immediate next action, listed in the
  landing note below rather than fixed here.
- `crates/kcode-task-types/src/lib.rs`: delete `TodoGoal`, `TodoPlan`, their
  change and field types, the enums only they used (`IntentUnderstanding`,
  `FeedbackLoopState`, `DeliveryState`, `Difficulty`, `Autonomy`,
  `IterationMaturity`, `FeedbackLoopRelevance`, `FeedbackLoopCoverage`,
  `FeedbackLoopTraceability`), `ConfidenceState`, and `TodoItem`'s three
  confidence fields.
- `kcode-base/src/todo.rs`: delete the goals and plan storage, the pass
  predicates, and the shrunk re-export list.
- Fixtures that construct `TodoItem` with the confidence fields (the `todo()`
  helpers in `todos_view.rs`, `turn_notify.rs`, `info_widget_tests.rs`,
  `ui_todo_changes.rs`) need `..Default::default()` or the fields dropped.
- Delete `internals/todo-calibration.md` and its `kcode-docs` path; re-measure
  the always-on tool cost in `../todo.md` §5, whose two worst offenders
  (`todo.feedback_loop_relevance`, `todo.feedback_loop_traceability`) are gone.

Two process findings from this step, both worth keeping:

- **`cargo clippy -- -D warnings` in the gate does not compile test code**, so a
  test-only breakage passes the gate. Run
  `cargo clippy -p <crate> --all-targets` and the suite itself before calling a
  landing done. One failure below was introduced in step 2 and stayed invisible
  through the gate plus a `cargo check --all-targets` run.
- **Beware of deleting by block scan.** Two of the edits here silently removed
  one line past the intended block (an `impl` opener, then a `fn` signature),
  because a scan that ends at the first column-zero `}` starts counting before
  the body opens when the signature spans lines. Both were caught by a compile,
  neither by review. Prefer explicit text anchors; verify with a build.

## Validation

The before/after probe recipe for comparing verification and poke count across
the rework lives in `dev/todo-rework-ab-probe.md`; this doc does not duplicate
it.

## Step 3 landing note (in progress, 2026-09-30)

State of the tree when this pass paused: `cargo check --all-targets` and
`cargo clippy --all-targets` are clean over the four crates, the whole guardrail
gate is green, and `kcode-tui --lib` single-threaded is **1800 passed, 7 failed**
(the run before the last display edits; those edits only removed readers, so the
count stands unless a fixture below changed). Six are this pass's, one is the
pre-existing failure recorded in `../todo.md`.

The six, each asserting behaviour this step deleted, with the fix each needs:

| Test | Asserts | Fix |
|---|---|---|
| `refresh_todo_card_updates_content_when_goal_scores_change` (`tests/todo_card.rs:164`) | the card payload still contains `"closed_feedback_loop"` | delete: the payload carries tasks only |
| `refresh_todo_card_updates_content_when_todos_change` (`tests/todo_card.rs:104`) | the payload contains `"goals"` | drop that one assertion; the rest of the test still holds |
| `test_context_command_reports_session_context_snapshot` (`tests/state_model_poke_02/part_01.rs:821`) | the snapshot prints `[pending\|high\|confidence plausible]` | expect `[pending\|high]` |
| `test_remote_non_retryable_error_stops_auto_poke_after_short_retry_budget` (`tests/remote_events_reload_04.rs:301`) | the queue is empty after the error clears auto-poke | the fixture pushes the poke as literal text; queue it with `commands::queue_poke_message` as production does. This one is a step-2 miss |
| `todos_view_hash_changes_when_confidence_changes` (`todos_view.rs`) | the card hash moves when `confidence` changes | delete: the hash no longer covers a removed field |
| `todos_widgets_render_group_headers_when_groups_present` (`info_widget_tests.rs:124`) | the group header shows `1/2 · confidence plausible` | drop that assertion; the header shows the counter |

The last attempt to apply these aborted on its first assertion (the fixture text
it searched for spans lines and had already been reformatted), so none of the
six were applied. They are the first thing to do when work resumes, before the
types are deleted.

## Desired final state (recorded 2026-09-30, not yet planned)

The maintainer's target for the todo feature, two stages past this rework.
Taken together they are what "the todo list is a display of the plan" becomes
when the display is a file a person can read.

**Stage 4: the list is a repository artifact.** One markdown file in the working
tree, written by the todo tool and readable and editable by the maintainer,
shared across sessions instead of one JSON file per session, versioned in git.
The widget and `/todos` read that file. `AGENTS.md` points at it so a session
starting cold finds the list without being told. This removes the duplication
that exists today between the session list and `docs/todo.md` by making them the
same artifact.

**Stage 5: the automation that advances the list writes to it.** Each item can
name the check that proves it, and turn-end work records the check's actual
result in the file, so `completed` means "the named check passed, and its output
is in the file" rather than "the model said so". A session starting cold can
then see what was verified and when, which is the anti-drift property: status
carries its own evidence and lives where the work lives.

What this changes about the stages above: step 3 still deletes the eight scored
dimensions, both confidence fields, and the plan fields. The one idea worth
carrying forward from that rubric is "name the check", and today it survives as
step 3's single line in the tool description. Stage 5 is where it becomes a
field again, at the item level, paired with a recorded result. It is not the
rubric returning: a rubric was a number the model wrote and the harness
thresholded, while this is a command whose output gets recorded.

Open questions for stage 4, deliberately not answered here: which repository and
branch a list belongs to, what two sessions working in one repository do, who
wins when the maintainer edits the file mid-session, and whether the per-session
file survives at all.

Constraints the maintainer settled on 2026-09-30, which narrow those questions:

- **The list is never hand-authored.** The human role is reading it and asking
  the model to change it, never writing it directly. So the tool can own the
  file outright: the write protocol is the tool's alone, and a human edit is just
  an input the tool re-reads on its next write. That removes the two-writer
  precedence problem, and it means the format only has to be readable and
  diff-stable, not pleasant to hand-edit.
- **A completed item is deleted**, as `docs/todo.md` already does ("Tick an item
  when it lands and delete it; git has the history"). So the durable record of
  what was done is the commit and the diff, and stage 5's recorded check matters
  while an item is open. The natural home for it at completion is the commit
  message the item's work lands in.

That also shrinks stage 5's scope: with completed items deleted, the list only
ever holds open work, so the anti-drift burden is "an open item that is actually
done", and not "a log whose entries have gone stale".

Phase split, as agreed: **phase 1 is steps 1-3**, all deletions. **Phase 2 is
stages 4-5**, both additions, with stage 6 following from stage 4. Do not start
phase 2 in phase 1's changes.

One constraint step 3 must respect, found while writing this: `TodoItem` is not
only the todo tool's type. It is also the display type for swarm plan items,
which `info_widget_todos::swarm_plan_todos` builds from `plan::PlanItem` and
which carry `blocked_by` and `assigned_to`, fields the todo tool never writes.
Shrinking `TodoItem` to the five display fields therefore needs either a separate
type for the plan display or those two fields kept on the shared one.

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
