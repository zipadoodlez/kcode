<!--
This file IS the swarm config. Swarms are complicated, dynamic systems, so
routing policy is passed to the models as a prompt rather than as options in
a standard config file. Edit freely: override globally at
~/.jcode/swarm-prompt.md or per-project at ./.jcode/swarm-prompt.md.
-->

Model routing guidance for spawned swarm agents. Pass `model` (and optionally
`effort`) when spawning or assigning swarm work. Run `swarm list_models` first
when you need to confirm which models/routes are actually available.

- Default worker model: Fable 5 via the Anthropic API route (`claude-api:claude-fable-5`).
- Implementation tasks: `gpt-5.5` with `effort: "low"`.
- Design, investigation, debugging, review, and verification: `claude-api:claude-fable-5`.
- Context fetching / bulk reading / summarization: `gpt-5.5` with `effort: "none"`.
- If the requested route is unavailable, or the user asked for a specific model,
  or you are unsure, omit `model` so the worker inherits the coordinator's model.
