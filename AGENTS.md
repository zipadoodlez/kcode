# Repository Guidelines

## Workflow

- Start at `docs/README.md`; `docs/plans/` holds the work, one lane per subject.
- Use the user's Git identity. Never substitute an agent name or address.
- Keep work scoped: your own branch, preserve unrelated work, and do not merge a PR or branch without the user's authorization.
- This fork tracks no upstream; there is nothing to fetch or rebase from `jcode`.
- Do not rename the remaining `jcode` names. The deliberate ones are listed in `docs/dev/post-change.md` under Names.
- Prove the change, and gate once. A deletion is proven by the build (a dangling reference fails compilation), a behavior change by the tests that cover it. While iterating, run only that proof, batched: `cargo clippy -p <crate> --all-targets --all-features -- -D warnings`, or the workspace when something shared changed; `scripts/test.sh crate <name>` for behavior. Run `scripts/check_guardrails.sh` and `scripts/test.sh full` once before a push, not after every commit.
- The suite is at zero single-threaded. At the default thread count a dozen tests flake on process-global `KCODE_*` state; re-run with `--test-threads=1` before believing a failure.
