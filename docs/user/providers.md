# Providers

kcode talks to many backends. This doc covers picking one; where its credential
lives is in [auth.md](auth.md).

## Picking a provider

```sh
kcode provider list       # ids you can pass to -p/--provider
kcode provider current    # what is requested and what actually resolved
kcode -p cerebras -m gpt-oss-120b
```

`-p/--provider` defaults to `auto`, which detects a provider from the
environment. In the TUI, `/model` switches provider and model at runtime. To
make a choice stick, set it in `~/.kcode/config.toml`:

```toml
[provider]
default_provider = "claude"
default_model = "claude-opus-4-8"
```

`provider list` prints the providers with a catalog entry; `-p` accepts more,
because aliases and gateways resolve to the same backends without their own
catalog row. When a name is rejected, `provider list` is the authority on what
works, not the flag's help text.

## Logging in

Every provider shares the same login shape:

```sh
kcode login --provider <id>
kcode login --provider <id> --no-browser        # headless / SSH
kcode login --provider <id> --print-auth-url    # scriptable: print a URL, finish later
```

Finish a printed flow with `--callback-url` (OpenAI, Claude, Antigravity) or
`--auth-code` (Claude, Gemini); the GitHub device flow (Copilot) resumes with
`--complete`. `--cancel --flow-id <id>` cancels one pending flow without touching
saved credentials. Native OAuth tokens land under `~/.kcode`:

| provider | command | credential |
|---|---|---|
| Claude (`claude`) | `kcode login --provider claude` | `~/.kcode/auth.json` |
| OpenAI (`openai`) | `kcode login --provider openai` | `~/.kcode/openai-auth.json` |
| Gemini (`gemini`) | `kcode login --provider gemini` | `~/.kcode/gemini_oauth.json` |
| Antigravity (`antigravity`) | `kcode login --provider antigravity` | `~/.kcode/antigravity_oauth.json` |

OpenAI's browser login listens on `http://localhost:1455/auth/callback` by
default and falls back to pasting the callback URL when the port is taken.
API-key providers (`anthropic-api`, `openai-api`, `bedrock`, `azure`, `cursor`,
`fireworks`, `novita`, `minimax`, `cerebras`, `groq`, `openrouter`, …) instead
store the key in `~/.config/kcode/<provider>.env`. If a trusted OpenCode/pi auth
file already holds a matching key, kcode reuses it after consent. See
[auth.md](auth.md) for the full credential model.

### Azure OpenAI

`kcode login --provider azure` asks for the endpoint
(`https://your-resource.openai.azure.com`), a deployment/model name, and an auth
mode: **Microsoft Entra ID** (recommended) or **API key**. Settings go to
`~/.config/kcode/azure-openai.env` (`AZURE_OPENAI_ENDPOINT`, `AZURE_OPENAI_MODEL`,
`AZURE_OPENAI_USE_ENTRA`, plus `AZURE_OPENAI_API_KEY` in key mode). Entra mode
resolves `DefaultAzureCredential` (run `az login` if it fails); key mode sends the
credential in the `api-key` header. Model catalog fetching is off for Azure, so
set the model explicitly.

### Experimental CLI providers

- `cursor` - native HTTPS; model override `JCODE_CURSOR_MODEL`.
- `copilot` - GitHub device flow; model override `JCODE_COPILOT_MODEL`.
- `antigravity` - native Google OAuth, no Antigravity install needed; model
  override `JCODE_ANTIGRAVITY_MODEL`.

### OpenAI endpoint override

For API-key use you can retarget the Responses API base with
`JCODE_OPENAI_API_BASE`, `OPENAI_BASE_URL`, or `OPENAI_API_BASE` (first set wins;
an absolute `http(s)://` base ending in the API version). kcode appends
`/responses` and derives the WebSocket and `/models` endpoints from it. The
override is ignored in ChatGPT/Codex OAuth mode, and a malformed value is logged
and ignored.

### Verifying a login

`kcode --provider <id> auth-test` runs credential discovery, a refresh probe, a
smoke prompt expecting `AUTH_TEST_OK`, then a tool-enabled smoke. Add
`--no-tool-smoke` to stop after the probes, or `kcode auth-test --all-configured`
to check every configured provider.

## OpenAI-compatible providers and custom gateways

Anything speaking the OpenAI API can be added as a named profile:

```sh
kcode provider add my-gateway \
  --base-url https://llm.example.com/v1 \
  --model llama-3.3-70b \
  --api-key-stdin            # or --api-key-env MY_GATEWAY_KEY, or --no-api-key
```

Then use it with `--provider-profile my-gateway`, or make it the default with
`--set-default`. Keys are stored in `~/.config/kcode/<provider>.env`; see
[auth.md](auth.md) for the two-path model (OAuth vs API key) and the app-config
location.

## AWS Bedrock

kcode has a native Bedrock provider using the AWS SDK's `ConverseStream`.

Two auth styles:

- **Bedrock API key / bearer token** - easiest locally. Stored in
  `~/.config/kcode/bedrock.env` and sent as `AWS_BEARER_TOKEN_BEDROCK`.
- **AWS IAM credentials** - the normal AWS path: a CLI/SSO profile, env access
  keys, web identity, or EC2/ECS metadata.

```sh
kcode login --provider bedrock          # guided: saves bearer token + region
kcode --provider bedrock --model anthropic.claude-3-5-sonnet-20241022-v2:0
kcode --model bedrock:us.anthropic.claude-sonnet-4-6
```

Relevant environment:

| variable | purpose |
|---|---|
| `AWS_BEARER_TOKEN_BEDROCK`, `AWS_REGION` | bearer-token auth |
| `AWS_PROFILE` | IAM/SSO profile |
| `JCODE_BEDROCK_PROFILE`, `JCODE_BEDROCK_REGION` | kcode-specific overrides |
| `JCODE_BEDROCK_ENABLE=1` | opt in to instance/container metadata credentials |
| `JCODE_BEDROCK_VALIDATE_STS=1` | validate with `sts:GetCallerIdentity` |
| `JCODE_BEDROCK_MAX_TOKENS`, `_TEMPERATURE`, `_TOP_P`, `_STOP_SEQUENCES` | per-request parameters |

Prefer an inference-profile ID such as `us.amazon.nova-2-lite-v1:0` over a bare
foundation-model ID when both exist; some models only invoke through a profile.
Model IDs and profiles change, so treat any example as an example and run
`/refresh-model-list` after changing region, enabling model access, or rotating
credentials. Discovery calls `ListFoundationModels` and `ListInferenceProfiles`
and caches region-scoped results. `/model` marks an unusable route `×` (not
selectable) or a limited one `⚠` (most often no tool use).

Minimum runtime IAM: `bedrock:InvokeModel` and
`bedrock:InvokeModelWithResponseStream`. Discovery adds
`bedrock:ListFoundationModels` and `bedrock:ListInferenceProfiles`. STS
validation adds `sts:GetCallerIdentity`. AccessDenied usually means model access
is not enabled in the AWS console; SSO token errors mean you need
`aws sso login --profile <profile>`.

## Conifer

Conifer uses the OpenAI-compatible runtime. Its authenticated `GET /v1/models`
catalog is authoritative for the current key: live or persisted catalog metadata
wins over built-in context limits, including when a live window is smaller.
kcode keeps a bundled fallback for route ids the public catalog does not resolve
(date-stamped observations, not model-family limits), and refresh replaces it.
When the list looks stale, run `/refresh-model-list`.

## Provider Doctor

`kcode provider-doctor <provider>` is the "why isn't my provider or model
picker working?" command. It walks the same strict end-to-end checkpoints the
coverage ledger tracks, but as an interactive run with pass/fail and a next step
on the first failure. It works for OpenAI-compatible providers and, on the
`full` tier, the native Anthropic/OpenAI subscription and API-key paths.

```sh
kcode provider-doctor cerebras --tier offline   # jcode wiring, no key, no spend
kcode provider-doctor cerebras --tier catalog   # live /models, needs key
kcode provider-doctor cerebras --tier full      # real chat, stream, tools, spends
kcode provider-doctor cerebras --model gpt-oss-120b --tier full --json
```

| tier | needs key | spends | adds |
|---|---|---|---|
| `offline` (jcode wiring) | no | no | picker rendering, catalog reload, fallback labeling, model-switch routing |
| `catalog` (default) | yes | ~none | live `GET /models`: bad/missing key, dead endpoint, model absent from the live catalog |
| `full` | yes | yes | real chat, streaming, the tool-call loop |

Only `full` can earn strict ("READY") coverage; lighter tiers record the
API-dependent checkpoints as skipped so nothing is over-credited. A failing
checkpoint names itself with a next step, and the command exits non-zero when
the tier did not fully pass, so it doubles as a CI gate. `--json` adds a `spend`
object (`billable_calls`, tokens, `reported_cost_usd`).

Every run records into the coverage ledger, and
`kcode provider-test-coverage` renders the same pipeline as one line per
provider/model with the first blocker and the exact doctor command to advance
it. The two commands are two views of one pipeline; the checkpoint list itself
is rendered by the command, not repeated here.
