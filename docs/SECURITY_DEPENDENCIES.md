# Dependency Security Triage

Last reviewed: 2026-03-05

This file tracks the current `cargo audit` findings for jcode and the intended remediation path.
It is not an allowlist. It is a triage record so advisories are visible and actionable.

## Current advisories

| Advisory | Crate | Dependency path | Affected area in jcode | Triage | Planned action |
|---|---|---|---|---|---|
| `RUSTSEC-2025-0141` | `bincode` | `syntect -> bincode` | Markdown/code highlighting in the TUI | Unmaintained transitive dependency. No direct exposure in the provider/auth flow. | Track `syntect` upgrades or replace `syntect` if upstream does not move off `bincode` soon. |
| `RUSTSEC-2024-0320` | `yaml-rust` | `syntect -> yaml-rust` | Markdown/code highlighting in the TUI | Unmaintained transitive dependency. Same containment as above. | Resolve together with the `syntect` upgrade/replacement work. |
| `RUSTSEC-2024-0436` | `paste` | `ratatui -> paste`, `tokenizers -> paste`, `tract-* -> paste` | TUI rendering, tokenizers, embedding/model support | Widely transitive. Not isolated to one module. | Prefer upstream dependency upgrades before any local workaround. Re-evaluate after bumping `ratatui`, `tokenizers`, and `tract-*`. |
| `RUSTSEC-2026-0002` | `lru` | `ratatui -> lru` | TUI rendering/cache internals | Unsoundness warning in a UI dependency. Not in auth/provider logic, but still ships in-process. | Upgrade `ratatui` / `ratatui-image` together once compatible. |
| `RUSTSEC-2023-0086` | `lexical-core` | `imap -> imap-proto -> lexical-core` | Gmail/IMAP support path | Old unsound transitive dependency in the mail stack. Higher priority than the UI-only findings because it touches network-parsed data. | Investigate upgrading or replacing `imap` / `imap-proto`. If no maintained path exists, isolate or remove the IMAP dependency. |

## Priority order

1. `lexical-core` via `imap-proto`
2. `lru` via `ratatui`
3. `bincode` / `yaml-rust` via `syntect`
4. `paste` via multiple transitive dependencies

## Notes

- None of the advisories above were introduced by the provider-auth refactor.
- The provider/auth hardening work should continue independently of these dependency upgrades.
- Before changing dependency versions, run:
  - `cargo check`
  - `cargo test -j 1`
  - `scripts/security_preflight.sh`

