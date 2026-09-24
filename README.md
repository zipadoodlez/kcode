<div align="center">

# kcode

A lean fork of the [jcode](https://github.com/1jehuang/jcode) coding agent.

</div>

kcode is the jcode TUI coding agent with a large amount of surface area
removed. Same TUI, same multi-model support, same tools and swarm
coordination, minus the parts this fork does not want to carry.

## What was removed

Roughly **241,000 lines across 1,079 files** were cut:

- mermaid rendering, diagrams, inline images, latex, session replay
- agent memory and ambient mode
- gmail and Google login, dictation, the productivity dashboard
- the macOS computer-use tool, the menubar app, the client installer
- the iOS app, the telemetry worker, the TypeScript SDK

The exact record, including every deleted path and the 44 cut commits, is in
[docs/plans/KCODE_CUT_MANIFEST.md](docs/plans/KCODE_CUT_MANIFEST.md).
The rename from jcode to kcode is documented in
[docs/plans/KCODE_RENAME.md](docs/plans/KCODE_RENAME.md).

## Build

```bash
cargo build --release -p kcode --bin kcode
```

## Install

```bash
install -Dm755 target/release/kcode ~/.local/bin/kcode
```

kcode keeps its own state, so it does not collide with an installed jcode:

| | |
|---|---|
| home | `~/.kcode` |
| config | `~/.kcode/config.toml` |
| app config | `~/.config/kcode` |
| sockets | `$XDG_RUNTIME_DIR/kcode.sock`, `kcode-debug.sock` |

## Run

```bash
kcode                 # launch the TUI
kcode run 'prompt'     # single non-interactive message
kcode server start     # background daemon
kcode server stop
```

## Relation to jcode

kcode is a fork. It is not tracking upstream commits; features removed here
will not come back automatically. Provider support and auth flows are inherited
from jcode and are the main thing to keep an eye on.

## Licence

Inherited from jcode. See [LICENSE](LICENSE).
