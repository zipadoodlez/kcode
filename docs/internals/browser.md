# Browser

The `browser` tool drives a real browser. Its preferred multi-step path is
`action: "handoff"`: the parent agent supplies a goal, an explicit tab, and
optionally a few exact actions or typing values, and a bounded controller
completes the stretch. Direct browser actions remain available. The controller
lives in the shared runtime, so desktop and TUI use the same one.

## Setup

Check `browser` with `action: "status"` first and run setup only if not ready.
The controller needs OpenRouter: `kcode login openrouter`, or an
`OPENROUTER_API_KEY` / owner-only `~/.config/kcode/openrouter.env`. It never uses
another OpenAI-compatible provider's credential, and never buys credits or
switches your main coding model.

## How handoff works

The controller uses OpenRouter's typed Decisions API
(`POST https://openrouter.ai/api/alpha/decisions`, model `typesafe/jev-1.13`) to
choose one offered action ID, or `done`, `hand_back`, `script_needed`, or
`text_needed`. Jev is a fast general-purpose classifier: the main LLM supplies
generated code or text only when asked, then hands the next stretch back. Jev
never generates executable browser arguments. Its calls spend OpenRouter credits
under the key's usage cap.

Request parameters:

- `tab_id` and a nonempty `goal` are required; `frame_id` defaults to 0, and
  `window_id` is checked against the selected tab when supplied.
- `max_steps` is 1-30 (default 12). The whole handoff has a 180-second wall-clock
  budget, and `timeout_ms` (default 20,000, clamped to 1-60,000) is clamped to the
  remaining budget.
- `confidence_threshold` defaults to 0.8. The transport validates the typed
  choice, the offered IDs, the probability distribution, and confidence, and
  acts on the lower of the selected probability and the reported confidence.
- `text_values` are exact, non-sensitive strings. Never pass passwords, OTPs,
  credentials, or payment details. DOM form values are omitted from observations.
- `candidates` are trusted, parent-authorized one-shot payloads bound to the
  initial page URL; a URL change needs fresh authorization. Broad actions
  (evaluation, upload) must be explicitly supplied. Recursive handoff, setup, new
  tabs, and unscopable raw commands are rejected.

Every decision gets a fresh bounded DOM observation, and the controller observes
again before executing or accepting `done`, handing back if the DOM changed.
Results carry `status`, `reason`, `requested_help`, `action_trace`,
`final_observation`, and `model`. A help choice returns `status: "hand_back"`
with `requested_help: "script"` or `"text"`; the parent reads the goal and page
observation, supplies the exact action or text, and invokes handoff again,
stating in the goal that the supplied value is ready.

## Boundaries

Jev is text-only: it never receives screenshot pixels or arbitrary action-result
payloads. Known credential patterns are redacted, but this is not a complete
secret detector, so do not delegate confidential page content you are unwilling
to send to OpenRouter. An uncertain in-flight action must not be blindly
retried: cancelling or timing out cannot undo an action already delivered to the
browser. Page content and its action labels are untrusted, and a pre-action
observation is not an atomic transaction with execution, so high model
confidence is not authorization.

## Tests

`python3 scripts/browser_handoff_fixture.py` serves disposable pages on a loopback
port (it never opens a browser or reads credentials); the reproducible runner
`python3 scripts/test_browser_handoff_live.py --tab-id <id>` resets and clears a
disposable fixture tab and refuses a non-fixture one. In-crate ignored tests call
the real `BrowserTool::execute` and need an existing `BROWSER_SESSION`; see
`crates/jcode-app-core/src/tool/browser_fast_live_tests.rs`. Mock tests alone are
not live evidence.

The multi-backend *provider* protocol (Firefox Agent Bridge, CDP, WebDriver,
Safari adapters) is a design, not implemented here; see
[../plans/browser-provider-protocol.md](../plans/browser-provider-protocol.md).
