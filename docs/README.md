# kcode Docs

Reference documentation for the kcode codebase.

## Layout

- `docs/*.md` — architecture, feature, and behavior docs (current state of the system).
- `docs/plans/` — forward-looking plans, roadmaps, and TODO trackers. May be partially implemented or stale.
- `docs/audits/` — point-in-time audits and reviews. Historical snapshots, not kept up to date.
- `docs/proposals/` — design proposals not yet committed to.
- `docs/dev/` — developer-facing process and testing notes.
- Docs superseded by the rewrite are moved out of the repo to
  `~/jcode-work/reports/retired-docs/` rather than deleted outright.

## Key entry points

- Architecture: `SERVER_ARCHITECTURE.md`
- Swarm: `SWARM_ARCHITECTURE.md`, `SWARM_TASK_GRAPH.md`
- Process memory: `PROCESS_MEMORY_BUDGET.md`, `PROCESS_MEMORY_INCIDENT_RUNBOOK.md`
- Providers: `PROVIDER_DOCTOR.md`, `AWS_BEDROCK_PROVIDER.md`
- Platform: `TERMINAL_CAPABILITIES.md`

## Conventions

- Docs describing current behavior live at the top level; anything speculative goes in `plans/` or `proposals/`.
- Prefer updating an existing doc over adding a near-duplicate.
- Root of the repo should only hold README, CONTRIBUTING, RELEASING, AGENTS, LICENSE, and similar meta files. Put everything else here.
