# Dependencies

Dependency advisories are triaged in one place: `scripts/security_preflight.sh`
runs `cargo audit` with an explicit ignore list. Anything not on that list fails
CI, so an ignore is a deliberate, dated decision rather than a blanket allowlist.

Before changing dependency versions:

```sh
cargo check
cargo test -j 1
scripts/security_preflight.sh
```

## Currently ignored (as of the last triage)

| advisory | crate | why it is ignored |
|---|---|---|
| `RUSTSEC-2026-0098/0099/0104/0049` | `rustls-webpki` (via `rustls`, `aws-smithy`) | TLS name-constraint and CRL issues; awaiting an upstream rustls/webpki bump (the `aws-sdk` stack needs a major bump). |
| `RUSTSEC-2026-0187` | `lopdf` (via `pdf-extract 0.8.2`) | Deeply nested PDFs can overflow the stack during text extraction. Only reached when extracting a PDF the user opens, never in the auth/provider/network path. `pdf-extract` pins `lopdf 0.34`; unblock when it depends on `>=0.42`. |
| `RUSTSEC-2026-0194/0195` | `quick-xml` (via `wayland-scanner`) | Build-time proc-macro parsing trusted, vendored Wayland protocol XML. No runtime untrusted input. |

## Resolved

- `RUSTSEC-2026-0217` (`tract-nnef`): fixed by moving `jcode-embedding` to
  `tract` 0.23 (the 0.21 line pinned an incompatible `half`).
- `RUSTSEC-2024-0320` (`yaml-rust`): removed from the graph by trimming `syntect`
  features to built-in syntax/theme dumps.
- `lettre` / `imap`: resolved by cutting the notification and mail integrations;
  neither crate remains.

The dependency *upgrade* work is independent of provider/auth hardening. The
authoritative ignore list is the script; this table is a summary and can lag it.
