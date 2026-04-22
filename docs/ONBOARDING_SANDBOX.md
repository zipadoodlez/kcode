# Onboarding sandbox

If you want to iterate on onboarding repeatedly without touching your real auth state, use a separate sandbox rooted under `JCODE_HOME` and `JCODE_RUNTIME_DIR`.

This repo already supports that isolation:

- `JCODE_HOME` redirects jcode-owned state such as `~/.jcode` into a sandbox directory.
- `JCODE_HOME` also redirects app config into `JCODE_HOME/config/jcode`.
- `JCODE_RUNTIME_DIR` redirects sockets and other ephemeral runtime files.
- External auth trust decisions are stored in the sandbox config, so a fresh sandbox starts with no trusted external auth imports.

## Fast start

```bash
scripts/onboarding_sandbox.sh fresh
```

That gives you a clean jcode launch with isolated state.

## Common commands

```bash
# Show the exact env vars and sandbox paths
scripts/onboarding_sandbox.sh env
scripts/onboarding_sandbox.sh status

# Start over from a blank onboarding state
scripts/onboarding_sandbox.sh reset
scripts/onboarding_sandbox.sh fresh

# Log into a provider without touching your normal jcode config
scripts/onboarding_sandbox.sh login openai
scripts/onboarding_sandbox.sh login claude
scripts/onboarding_sandbox.sh auth-status

# Run arbitrary jcode commands in the sandbox
scripts/onboarding_sandbox.sh jcode auth status
scripts/onboarding_sandbox.sh jcode pair
```

## Mobile onboarding simulator

The repo also has a resettable headless mobile simulator with predefined onboarding scenarios.

```bash
# Start the simulator in the background
scripts/onboarding_sandbox.sh mobile-start onboarding

# Inspect it
scripts/onboarding_sandbox.sh mobile-status
scripts/onboarding_sandbox.sh mobile-state
scripts/onboarding_sandbox.sh mobile-log

# Reset it back to the scenario start
scripts/onboarding_sandbox.sh mobile-reset
```

Supported scenarios today:

- `onboarding`
- `pairing_ready`
- `connected_chat`

## Why this is safer

A fresh sandbox means:

- no real jcode config files are reused
- no real runtime sockets are reused
- no previously trusted external auth sources are reused
- you can blow it away with one `reset`

## Recommended workflow

For tight onboarding iteration, use this loop:

1. `scripts/onboarding_sandbox.sh reset`
2. `scripts/onboarding_sandbox.sh fresh`
3. walk the onboarding flow
4. adjust code
5. repeat

If you are iterating specifically on mobile onboarding UX, keep the simulator running and use `mobile-reset` between passes.

## Caveat

This sandbox is designed to isolate jcode-owned state and trusted external-import state. If you later decide to test explicit import/reuse flows from external tools, do that intentionally and treat it as a separate test case from first-run onboarding.
