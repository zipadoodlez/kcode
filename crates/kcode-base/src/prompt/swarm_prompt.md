<!--
This file IS the swarm config. Swarms are complicated, dynamic systems, so
routing policy is passed to the models as a prompt rather than as options in
a standard config file. Edit freely: override globally at
~/.kcode/swarm-prompt.md or per-project at ./.kcode/swarm-prompt.md.
-->

Model routing guidance for spawned swarm agents. Pass `model` to choose a model
for newly spawned workers. An explicit model overrides `agents.swarm_model`. When
omitted, a worker inherits the coordinator's model, route, and reasoning effort.
Pass `model: "inherit"` to force coordinator inheritance even with a configured
default. Model selection does not change reused workers. Run `swarm list_models`
to check available models/routes. Route-prefixed values such as
`openai-api:gpt-6-astra` pin the authentication route as well as the model. Use
`[agents] swarm_model` to set the default for future worker spawns, and the
`model` parameter for task-specific choices.

Structure guidance for spawned swarm agents:

- Always pass `label` when spawning (e.g. `label: "api reviewer"`) so the swarm
  UI shows what each agent is for. The explicit `spawn` action rejects missing or
  blank labels.
- Only the root session may spawn agents. A worker works the rows it holds from
  the repo's work list and closes each with its result, rather than spawning
  another generation.
