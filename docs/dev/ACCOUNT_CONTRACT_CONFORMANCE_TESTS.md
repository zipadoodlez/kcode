# Account Contract Conformance Tests and Test Vectors

Executable conformance design for the jcode subscription account contract:
device login, browser approval/denial, account state (`/v1/me`), checkout,
billing portal, webhook ordering, revocation, and mixed-version compatibility.

The client half of the contract lives in this repo; the server half lives in
the private `solosystems-backend` repo. This document defines the shared test
vectors, the harnesses that execute them on each side, and who owns what.

## Grounding (current code)

- Device flow client and wire contract: `src/cli/login/jcode_device.rs:1-225`
  - `POST {auth_base}/v1/auth/device {"email"}` ->
    `{device_code, verify_url, expires_in (default 900), interval (default 5)}`
  - `POST {auth_base}/v1/auth/token {"device_code"}` ->
    `202`/`428` pending, `429` slow-down, `200` approved
    `{api_key, account_id?, email?, tier?}`, `200 {"status":"pending"}` legacy
    pending, error codes `authorization_pending|pending`, `slow_down`,
    `expired_token|expired|expired_device_code`, `access_denied|denied`,
    and `404`/`410` treated as expired.
- Poll loop timing: `src/cli/login/jcode_device.rs:230-264`
  (`interval.max(1)`, deadline from `expires_in`, `slow_down` adds 5s).
- Credential persistence: `src/cli/login/jcode_device.rs:268-304`
  (`JCODE_API_KEY`, `JCODE_ACCOUNT_ID`, `JCODE_ACCOUNT_EMAIL`, `JCODE_TIER`
  into the jcode-subscription env file).
- Account state client: `crates/jcode-base/src/subscription_api.rs`
  (`GET /v1/me` -> `SubscriptionMe`, 5s timeout, persists cached tier;
  unknown/absent tier gates like Plus:
  `crates/jcode-base/src/subscription_catalog.rs:193-204`).
- Existing executable harness to extend: scripted local HTTP server in
  `src/cli/login/jcode_device/tests.rs:7-46` plus state-machine tests
  (`poll_state_machine_*`, `poll_for_api_key_*`).
- Auth failure classification for negative-path assertions:
  `crates/jcode-base/src/auth/login_diagnostics.rs`.

## Repository ownership

| Concern | Owner repo | Harness |
|---|---|---|
| Client poll state machine, persistence, `/v1/me` parsing, tier gating | jcode (this repo) | Rust unit/integration tests against scripted HTTP server |
| Shared wire test vectors (JSON fixtures) | jcode, mirrored into solosystems-backend by version tag | `tests/fixtures/account-contract/` (proposed) |
| Email delivery, approval/denial web page, checkout session creation, Stripe webhooks, key revocation, `/v1/me` truth | solosystems-backend (private) | Backend integration tests replaying the same fixtures against real handlers |
| End-to-end smoke (live staging) | solosystems-backend CI, opt-in job in jcode CI gated on staging creds | `jcode login jcode` scriptable flow against staging `JCODE_API_BASE` |

Rule: a fixture change is a contract change. Fixtures are versioned
(`schema_version` field per vector file); both repos pin the fixture set and a
mixed-version matrix (below) proves old clients still pass against new server
vectors and vice versa.

## Fixture layout (proposed, this repo)

```
tests/fixtures/account-contract/
  v1/
    device_auth/           # responses to POST /v1/auth/device
    token_poll/            # scripted sequences for POST /v1/auth/token
    me/                    # GET /v1/me bodies
    webhook_order/         # backend-only, mirrored for documentation
    manifest.json          # {schema_version, vectors: [...]}
```

Each vector: `{name, request, response_script: [(status, body)...],
expected_outcome, notes}`. The Rust harness deserializes the manifest and
drives `spawn_scripted_http_server` so vectors are data, not code.

## 1. Device login vectors

| ID | Script | Expected |
|---|---|---|
| DL-01 happy path | device 200 full body; token 202, 200 approved | `TokenApprovedState` populated; env file has all four keys |
| DL-02 defaults | device 200 without `expires_in`/`interval` | defaults 900/5 applied |
| DL-03 legacy pending | token 200 `{"status":"pending"}` then approved | pending classified, then approved |
| DL-04 nested error | token 400 `{"error":{"code":"authorization_pending"}}` | Pending |
| DL-05 flat OAuth error | token 400 `{"error":"slow_down"}` | SlowDown, wait += 5s |
| DL-06 expired | token 400 `expired_token` (also `expired`, `expired_device_code`) | Expired, clear rerun message |
| DL-07 gone | token 404 / 410 with empty body | Expired |
| DL-08 denied | token 403 `{"error":{"code":"access_denied","message":"..."}}` | Denied with server message surfaced |
| DL-09 empty api_key | token 200 `{"api_key":"  "}` | hard error, nothing persisted |
| DL-10 garbage 200 | token 200 non-JSON | parse error, nothing persisted |
| DL-11 unexpected 5xx | token 500 | error includes status + trimmed body |
| DL-12 device reject | device 400/422/429 | login aborts before any poll |

## 2. Browser approval/denial (backend-owned, vector-mirrored)

Client cannot test the web page; the backend must have executable tests for:

- BA-01 approve link marks device_code approved exactly once (idempotent).
- BA-02 deny link yields `access_denied` on next poll with the denial reason.
- BA-03 approving an expired code returns an error page, poll stays Expired.
- BA-04 the magic-link token is single-use: second click is a no-op/error.
- BA-05 approval from a different account/session than the email target fails.
- BA-06 `verify_url` host must match the auth service origin (client-side
  negative: reject/refuse to auto-open non-HTTPS or foreign-origin URLs; today
  `maybe_open_browser` opens whatever the server sends — add this check).

## 3. Account state (`/v1/me`)

| ID | Body | Expected |
|---|---|---|
| ME-01 full | active flagship w/ usage | parsed, tier cached |
| ME-02 minimal | missing `resets_at`, unknown tier `"mystery"` | tolerated; `parsed_tier()` None; gating falls back to Plus |
| ME-03 401 | `{"error":"invalid_key"}` | error surfaced; cached tier NOT overwritten (revocation is explicit, see 6) |
| ME-04 5xx/timeout | delay > 5s | `ME_FETCH_TIMEOUT` fires; offline gating uses cached tier |
| ME-05 status values | `active`, `past_due`, `canceled`, `trialing` | client renders status verbatim; no crash on unknown |

## 4. Checkout and portal

Checkout/portal are web-only today; the client hands off at
`JCODE_PRICING_URL` (`src/cli/login/jcode_device.rs:355-367`). Conformance:

- CK-01 (backend) creating a checkout session for a signed-in device links the
  resulting subscription to the same `account_id` the device login returned.
- CK-02 (backend) completed checkout updates `/v1/me` tier within N seconds;
  vector asserts eventual consistency bound (suggest N=60 for staging test).
- CK-03 (client) after checkout, a fresh `/v1/me` fetch upgrades cached tier
  without re-login (test: ME-01 with new tier over old cached value).
- CK-04 (backend) portal cancel flows set `status:"canceled"` while keeping
  the key valid until period end; client vector ME-05 covers rendering.
- CK-05 (client) tier==none/empty after login prints the pricing prompt
  (`login_jcode_device_flow` tail) — snapshot test on stderr text.

## 5. Webhook ordering (backend-owned)

Stripe delivers webhooks out of order and at-least-once. Backend tests must
replay these orderings against the webhook handler and assert final state:

- WH-01 `checkout.session.completed` then `invoice.paid` (normal).
- WH-02 `invoice.paid` before `checkout.session.completed` (reorder).
- WH-03 duplicate delivery of each event (idempotency keys).
- WH-04 `customer.subscription.deleted` racing a same-second `invoice.paid`:
  terminal states win by event `created` timestamp, not arrival order.
- WH-05 signature invalid / stale timestamp -> 400, no state change.
- WH-06 unknown event type -> 2xx ack, no state change (forward compat).

Client-observable contract: after any WH sequence settles, `/v1/me` reflects
exactly one coherent `{tier, status}`; mirrored fixtures in `me/` enumerate
the reachable final states so the client test matrix stays closed.

## 6. Revocation

- RV-01 (backend) portal/admin revocation invalidates the API key: model API
  and `/v1/me` return 401 within a bounded lag (assert <= 60s in staging).
- RV-02 (client) 401 from the model API classifies as an auth failure with a
  recovery hint pointing at `/login jcode`
  (`crates/jcode-base/src/auth/login_diagnostics.rs`).
- RV-03 (client) revoked key must not silently fall back to another provider
  without surfacing the auth failure (account failover tests in
  `crates/jcode-base/src/provider/account_failover.rs`).
- RV-04 (backend) re-login after revocation issues a NEW key; old key stays
  dead (no resurrection).

## 7. Mixed-version compatibility matrix

Run the DL/ME vector suites in a 2x2 matrix:

| | old vectors (v1) | new vectors (v1.x) |
|---|---|---|
| released client (stable channel) | must pass | must pass ignoring unknown fields |
| head client | must pass | must pass |

Rules encoded as tests: unknown JSON fields ignored (serde default behavior —
add `deny_unknown_fields` NEVER); absent optional fields default (DL-02,
ME-02); new error codes fall into the "unexpected error" branch with the raw
body preserved (DL-11) rather than being misclassified as pending.

## 8. Security negative tests

- SN-01 device_code entropy: backend test asserts >= 128 bits, not guessable
  sequential IDs; token endpoint rate-limits per code and per IP (429 path is
  already client-handled: DL-05).
- SN-02 email enumeration: `/v1/auth/device` returns the same shape for known
  and unknown emails (backend).
- SN-03 client never prints `api_key` or full `device_code` to stdout/stderr
  or logs (grep-based test over captured output of the login flow; see the
  observability doc's never-log list).
- SN-04 HTTP (non-TLS) `auth_base` refused outside tests unless
  `127.0.0.1`/`localhost` (client change + test; today any base is accepted).
- SN-05 oversized/hostile bodies: 10 MB body, wrong content-type, NUL bytes —
  client errors cleanly, no panic (fuzz-style vectors in `token_poll/`).
- SN-06 env-file permissions: persisted credentials file is 0600 on Unix
  (test on `persist_subscription_credentials`).
- SN-07 verify_url scheme/host allowlist before auto-opening browser (BA-06).
- SN-08 poll after approval: reusing a consumed device_code returns expired,
  never a second key (backend; client covered by DL-07 semantics).

## 9. Clocks and races

- CR-01 `interval: 0` -> clamped to 1s (unit test exists implicitly via
  `interval.max(1)`; make it explicit).
- CR-02 `expires_in: 0` -> deadline is `max(expires_in, interval)`; loop
  terminates with expiry error, no hot spin.
- CR-03 repeated `slow_down` grows wait monotonically; cap total at deadline.
- CR-04 approval lands between deadline check and poll: client accepts the
  approved response even if past deadline check happens next iteration only —
  vector: pending until t=deadline-1, then approved.
- CR-05 client timing uses `Instant` (monotonic), so wall-clock skew must not
  matter: test with mocked large `expires_in` and manual outcome injection.
- CR-06 two concurrent logins for the same email: last writer wins on the env
  file; no interleaved/corrupt file (serialize via file lock or accept and
  document last-write-wins with a test).
- CR-07 backend: approve and expire racing at the same second — exactly one
  outcome persisted.

## Execution plan

1. Add `tests/fixtures/account-contract/v1/` with the DL/ME vectors above and
   a manifest; port `spawn_scripted_http_server` into a shared test util.
2. Convert existing `jcode_device/tests.rs` cases to load from the manifest,
   keeping current assertions (no behavior change).
3. Add the client-side gaps found while writing this spec: SN-03, SN-04,
   SN-06, SN-07, CR-01/02/06, ME-03 cache-preservation.
4. Mirror `manifest.json` into solosystems-backend and wire the backend suites
   (BA, WH, RV, CK, SN-01/02/08, CR-07) there.
5. Add the mixed-version CI job: run stable-channel binary's login flow
   against head fixtures via scripted server.
