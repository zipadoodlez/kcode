# Todo and goal calibration

Reference for the `todo` tool's plan and goal fields. The always-on schema
carries only short summaries and the enum values; the detail lives here and is
read on demand.

## Intent (`plan.understands_user_intent`)

Build a requirement inventory from the request: outcomes, deliverables,
constraints, prohibited actions, integration paths, edge cases, and necessary
follow-through. Make the plan and its stated user intention represent every
material item. Resolve what you can yourself; ask only when a decision is
genuinely the user's to make.

## Feedback-loop relevance

How directly the checks represent observable acceptance behavior:

- `indirect` - inspection or an internal proxy.
- `synthetic` - custom harnesses, stubs, mocks, copied sources, or synthetic
  fixtures.
- `representative` - real public interfaces, but not the complete acceptance
  workflow.
- `acceptance_blocked` - the real acceptance workflow was attempted but an
  external constraint prevented a result.
- `acceptance_aligned` - the real project build, integration test, or end-user
  workflow passed.

Substitute-only validation is never `acceptance_aligned`.

## Feedback-loop traceability

How completely requirements map to evidence:

- `unmapped` - requirements are not tied to checks.
- `partial` - only some explicit requirements or changed public outputs have
  concrete checks and observed results.
- `complete` - every explicit requirement and changed public output has a
  concrete check and observed result.

Aggregate test counts alone do not establish complete traceability.

## Feedback-loop coverage

How broadly the checks exercise main workflows, integration boundaries, edge
cases, packaging, and likely failure modes.
