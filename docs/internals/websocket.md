# OpenAI WebSocket transport

The native OpenAI providers (`openai` for the ChatGPT subscription, `openai-api`
for an API key) prefer a persistent Responses WebSocket in `auto` transport mode.
Every new socket, including a prewarm socket, sends:

```text
OpenAI-Beta: responses_websockets=2026-02-06
```

That header selects the WebSocket **v2 protocol**. It is not a `/v2/responses`
endpoint: API-key requests still use `/v1/responses` (or the configured
Responses API base), ChatGPT/Codex OAuth requests use the subscription Responses
backend, and custom Chat Completions providers are unaffected.

## Prewarming

When an idle client subscribes, kcode snapshots its tools and static
instructions and prepares them in the background while the user types. Preparation
uses `response.create` with `generate: false`, empty `input`, and `store: false`;
the server returns a completed response ID with no model output. If warmup
finishes before generation is needed, kcode continues on that socket with
`previous_response_id` and the real conversation input.

- Warmup never executes tools or emits assistant output.
- The foreground request never waits for an unfinished warmup: it cancels it and
  takes the ordinary connection path.
- Every request setting must match - model, instructions, tools, reasoning effort,
  service tier, cache policy, credentials, endpoint.
- An existing conversation socket takes precedence over warmup.
- Warmup has a 5-second timeout; unused state expires after 30 seconds. Model,
  credential, and transport resets discard speculative state, and forks do not
  inherit a parent's warmup socket.
- Expiring credentials skip warmup. Speculation never rotates OAuth refresh
  tokens, so cancelling cannot discard newly issued credentials.
- Warmup errors do not fail the user's request or trigger a transport cooldown;
  ordinary WebSocket recovery and HTTPS fallback still apply.

The benefit depends on having preparation time to overlap with network work; a
warmup miss is not a failure and there is no guaranteed speedup.

## Controls and diagnostics

```toml
[provider]
openai_transport = "auto"   # auto | websocket | https
```

Prewarming is on by default for native OpenAI WebSockets. `JCODE_OPENAI_PREWARM=0`
(`false`/`off`) in the **server process** environment disables speculative warmup
without disabling persistent WebSockets; `openai_transport = "https"` disables
both. The provider's diagnostic summary reports `websocket_protocol=v2`, and
lifecycle logs include `ws_prewarm_ready`, `ws_prewarm_hit`, `ws_prewarm_miss`,
and `ws_prewarm_unavailable`. A hit uses the `websocket/persistent-reuse`
connection label. Diagnostics do not include credential identities or warmup
inputs.

Native `response.steer` and multiplexed `stream_id` are separate features and are
not implemented here.

## Tests

```sh
cargo test -p jcode-provider-openai-runtime --lib -- --test-threads=1
cargo test -p jcode-provider-openai-runtime --lib \
  live_openai_v2_prewarm_and_continuation -- --ignored --nocapture --test-threads=1
```

The second is opt-in, uses configured credentials, and checks cold, warmed, and
continued responses. Its single-sample timings are not a benchmark.
