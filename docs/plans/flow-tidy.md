# The handback flow

The handback is the row's close: a worker's words are the close's `result`, and
the record lands on the row's parent, where the coordinator reads it. The engine
and the wording both landed with `3fcc9e1a`; the 2026-10-04 fan-out run (clover)
ran a build that had the engine and the old description, so its coordinator read
four worker transcripts out of `~/.kcode/sessions` by hand instead of reading
rows. What is left is not the mechanic. It is the edges that run exposed: a bound
on the words, a home for the long form, and the two errors it hit twice each.

## Tasks

- [ ] **Cap the close's `result`.** `close_row` (`kcode-base/src/todo.rs:219`)
 trims the result and stores it verbatim, `records` has no bound, and the list
 read (`tool/todo.rs:177`) pretty-prints every record. So the shared list is only
 as small as the longest report a model chooses to write. Reject a `result` over
 a small cap (start at ~400 chars, trimmed) with an error that names the escape
 (write the long form to a file), because a bare refusal invites a silent retry.
 Trap: count `chars()`, not bytes, to match `truncate_detail`.
 Proof: a close over the cap is refused with the file instruction; one at the cap
 lands and reads back.

- [ ] **Name where the long form goes.** The close instruction
 (`tool/todo.rs:240`) asks only for the proof, and the artifact field that once
 held a deliverable was removed (`eb721548`), so an investigation row has nowhere
 to put findings but `result`. One sentence in the instruction: write the long
 form to a file and name the file in the result. It reuses the repo's own files
 (`docs/plans`, the scratch dir) and adds no field.
 Proof: the description carries the sentence, and a test asserts it.

- [ ] **(decision) File pointer, or a long field read on demand?** The pointer
 keeps the list read cheap and adds no state, at the cost of one file hop on the
 rare long row. A long field, omitted from the list and returned only by a direct
 read, saves the hop and is new storage for a case only investigation rows have.
 Recommendation: the pointer.

- [ ] **The batch error names the call it is about.** `batch` requires `tool` on
 every sub-call (`tool/batch.rs:45`), and a missing one dies in serde as
 ``missing field `tool` ``, naming no call. The run hit this once per fan-out and
 re-sent four prompts each time. `normalize_batch_input` (`batch.rs:132`) already
 walks the sub-calls, so detect the missing `tool` there and error with the index
 and the live tool names. Add a swarm call to `BATCH_DESCRIPTION` (`batch.rs:12`),
 whose only example is `read`/`kgrep`.
 Proof: a batch whose first sub-call lacks `tool` errors naming `tool_calls[0]`;
 the description test names the swarm example.

- [ ] **(decision) The anchor's own close keeps no record.** `close_row` writes
 the record only when the row has a parent (`todo.rs:249`). A run's anchor is
 created or promoted per run (`anchor_from_words`, `anchor_from_rows`, reached
 from `live_turn.rs:160,355`), and a run's other root rows reparent to it, so a
 fan-out's records collect on the anchor. The anchor itself has no parent, so when
 it closes its words are stored nowhere. That is correct when the coordinator
 holds the anchor, because its synthesis is those words, and a loss when a worker
 holds a lone root row that becomes its own anchor. Pick: document "the
 coordinator holds the anchor" (free), or keep the anchor's close too (work).

- [ ] **(maybe) A member that ends holding an open row.** The member-ready
 notification site exists (`server/swarm.rs:1092`), so a line there could name a
 worker that stopped with rows still open. But a row the run has worked once is
 not picked again (`live_turn.rs:459`) and stays visible in the list, so the open
 row is already the signal the coordinator reads on its next list. Take this only
 if a by-hand run shows a coordinator missing it.

## Gate

The lane is done when a fresh fan-out run on the next build has its coordinator
read every report from the row list and open no session file, and when an
over-long close is refused with the file instruction instead of stored.
