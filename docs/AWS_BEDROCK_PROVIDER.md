# AWS Bedrock provider

Jcode supports a native AWS Bedrock provider that talks directly to Bedrock Runtime with the AWS Rust SDK and `ConverseStream`.

## Configure credentials

Use normal AWS credential mechanisms, or a Bedrock API key:

```bash
jcode login --provider bedrock
```

This saves `AWS_BEARER_TOKEN_BEDROCK` and `JCODE_BEDROCK_REGION` to `~/.config/jcode/bedrock.env`.

You can also configure manually:

```bash
export AWS_BEARER_TOKEN_BEDROCK=your-bedrock-api-key
export AWS_REGION=us-east-1
```

For IAM/SSO credentials:

```bash
export AWS_PROFILE=my-profile
export AWS_REGION=us-east-1
# Optional Jcode-specific overrides:
export JCODE_BEDROCK_PROFILE=my-profile
export JCODE_BEDROCK_REGION=us-east-1
```

If you rely on instance/container metadata credentials and have no local profile env vars, opt in explicitly:

```bash
export JCODE_BEDROCK_ENABLE=1
export AWS_REGION=us-east-1
```

For AWS SSO profiles, run:

```bash
aws sso login --profile my-profile
```

## IAM permissions

The runtime path needs, at minimum:

```json
{
  "Effect": "Allow",
  "Action": [
    "bedrock:InvokeModel",
    "bedrock:InvokeModelWithResponseStream"
  ],
  "Resource": "*"
}
```

Model discovery additionally uses:

```json
{
  "Effect": "Allow",
  "Action": [
    "bedrock:ListFoundationModels",
    "bedrock:ListInferenceProfiles"
  ],
  "Resource": "*"
}
```

If you enable STS validation with `JCODE_BEDROCK_VALIDATE_STS=1`, allow `sts:GetCallerIdentity`.

## Run Jcode with Bedrock

```bash
jcode --provider bedrock --model anthropic.claude-3-5-sonnet-20241022-v2:0
```

or:

```bash
jcode --model bedrock:anthropic.claude-3-5-sonnet-20241022-v2:0
```

Inference profile IDs/ARNs are accepted as model IDs, for example:

```bash
jcode --model bedrock:us.anthropic.claude-3-5-sonnet-20241022-v2:0
```

## Optional request parameters

```bash
export JCODE_BEDROCK_MAX_TOKENS=4096
export JCODE_BEDROCK_TEMPERATURE=0.2
export JCODE_BEDROCK_TOP_P=0.9
export JCODE_BEDROCK_STOP_SEQUENCES='</done>,STOP'
```

## Model discovery

Jcode will use a static Bedrock model list immediately. When model prefetch/catalog refresh runs, it calls `ListFoundationModels` and `ListInferenceProfiles`, then caches results in Jcode's config directory.

## Live smoke test

The live test is ignored by default. Run it only with valid AWS credentials and enabled model access:

```bash
JCODE_BEDROCK_LIVE_TEST=1 \
AWS_PROFILE=my-profile \
AWS_REGION=us-east-1 \
cargo test -p jcode --lib provider::bedrock::tests::bedrock_live_smoke_test -- --ignored
```

## Troubleshooting

- `AccessDenied`: grant Bedrock invoke/list permissions and enable model access in the AWS Console.
- `model not found` or validation errors: verify model ID/inference profile and region support.
- SSO token errors: run `aws sso login --profile <profile>`.
- API key auth: set `AWS_BEARER_TOKEN_BEDROCK` and `AWS_REGION`.
- Missing region: set `AWS_REGION` or `JCODE_BEDROCK_REGION`.
