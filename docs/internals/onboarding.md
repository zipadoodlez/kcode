# Onboarding

Onboarding is not one flow; it is a product of independent state spaces (UI
phase, per-provider credential state, environment capability, import
candidates, transport). Treating it as a state space rather than a pile of
conditionals is what makes the cross-axis bugs unrepresentable. Two pieces of
that model are shipped.

## Credential lifecycle

`auth::refresh_state::CredState` gives every OAuth provider the same lifecycle:

- `Absent` (no credential), `Present` (exists, never observed working),
  `Verified` (a refresh succeeded and nothing failed since), `Stale` (a refresh
  failed transiently, worth retrying), and the terminal
  `Rejected(fingerprint)` / `Unusable(reason)`.

`Rejected` is **terminal for that credential fingerprint**: no background sweep,
catalog refresh, or retry may touch it. Only a real re-login (a new fingerprint)
clears it. The fingerprint is a short hash of the refresh token, so the token
itself is never stored for this. Every provider records outcomes through
`record_refresh_outcome`, and callers gate a round-trip with
`ensure_refresh_allowed`, so no provider can silently opt out of terminal
rejection.

User-facing labels are a `match` on this enum, in one place, so a provider that
is merely `not_configured` can no longer render as "login expired".

## Environment facts

`auth::env_facts` probes, concurrently and cheaply, whether the machine has an
interactive tty, a browser, a bindable loopback port, a writable config dir,
network, a sane clock, a keyring, a proxy, and a container environment - each as
`Yes`/`No`/`Unknown`. `Unknown` biases toward the optimistic path.

This replaces "discover the capability by failing": a machine that positively
cannot use a browser (`browser_suppressed`) skips straight to a device-code or
paste-callback flow instead of burning the user's first 90 seconds on a callback
that was never going to arrive.

## The state graph

`crates/jcode-tui/src/tui/app/onboarding_graph.rs` declares the flow as data,
including states the flow always had but never modelled (`EnvBlocked`,
`LoginFailed`, `CredRejected`), and `check_invariants` enforces properties over
the whole graph rather than leaving them to review:

- no dead ends: every non-terminal node has an outgoing edge the user can press;
- every failure node has a recovery edge that is not "restart kcode";
- bounded steps and keystrokes to ready;
- every node reachable under some environment, and anything reachable under none
  is dead code;
- an `Esc`/skip escape hatch everywhere;
- no cycle without a user-visible state change;
- no effect targets a provider in `Rejected`.

This runs in CI via `scripts/check_guardrails.sh` (`onboarding state-space
invariants`), and `crates/jcode-tui/src/tui/app/tests/onboarding_eval.rs` scores
the flow's path budget. The graph's remaining follow-on work (extracting the
transition table and an effect-interpreter split) is tracked in [../wip.md](../wip.md).

## Trying it locally

Use an isolated sandbox (`scripts/onboarding_sandbox.sh`) so repeated onboarding
runs never touch real auth state; see [../dev/testing.md](../dev/testing.md).
