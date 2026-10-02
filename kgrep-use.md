# kgrep use and discoverability

Findings from a session of questions, 2026-09-29. No code was changed.
Working tree `80e3644c`. Running binary is `cd795377`.

## Verdict

Recognition is clunky, structurally rather than cosmetically. The model never
receives a signal connecting its prior ("search = grep or rg") to the tool it
should call. The inbound alias layer works. The outbound discovery layer does
not.

## The pipeline that decides tool choice

Four text surfaces decide whether a model picks `kgrep`:

1. Tool name: registry advertises `kgrep`
   (`crates/kcode-app-core/src/tool/mod.rs:342`). No `grep` tool and no `glob`
   tool are registered anywhere.
2. Tool description (`crates/kcode-app-core/src/tool/kgrep.rs:78`):
   `"Search code and file names. Defaults to grep mode when mode is omitted."`
3. Schema: 12 properties, no `required` array, `mode` defaults to `grep`,
   `query` conditionally required, three budget knobs (`max_files`,
   `max_regions`, `max_tokens`).
4. System prompt (`crates/kcode-base/src/prompt/system_prompt.md`, 35 lines):
   names `todo`, `open`, `panel`, skills. Never names a search tool.

## What works

`crates/kcode-tool-core/src/lib.rs:385,396` resolves inbound names:
`grep`, `file_grep`, `Grep` -> `kgrep`. `kgrep/args.rs:26,38` accepts `pattern`
and `include` as aliases.

This is a second-step mechanism. It fires only after a model has already
decided to call something named grep. It cannot help discovery, and discovery
is the step that fails.

## Clunks, ranked

1. **Advertised name is not the token models reach for.** `kgrep` is not a
   trained verb. Name is the strongest cue a model has.
2. **Description undersells.** No capability named (regex, glob filter, type
   filter, find, outline, trace), no "only search tool", no ripgrep. The word
   ripgrep appears only in a nested param (`kgrep.rs:119`).
3. **No prompt line steers search.** The prompt sets tool policy for `todo`,
   `open`, `panel`. Search has none, though the precedent exists.
4. **`bash` is an unguarded escape hatch.** Nothing warns that `rg` is absent.
   On this machine `which rg` fails and `rg -n pattern .` exits 127 with empty
   stdout. An empty result reads as "no matches". This is a correctness risk,
   not just style. It caused one wrong conclusion in this very session.
5. **One concept, five spellings.**

   | surface | says |
   |---|---|
   | `tool/mod.rs:342` | `kgrep` |
   | `kcode-tool-core/src/lib.rs:385` | `grep`, `file_grep` -> `kgrep` |
   | `kcode-tool-core/src/lib.rs:396` | `Grep` -> `kgrep` |
   | `kcode-provider-anthropic/src/lib.rs:484` | curated `Grep`, "built on ripgrep", unreachable |
   | `src/cli/acp.rs:1843,1854` | `glob`, `grep` mapped beside `kgrep` |
   | `config/default_file.rs:277` | doc says "kgrep, glob, grep, and ls" |

   The real minimal profile (`config.rs:665`) lists neither glob nor grep.

6. **Anthropic OAuth route sees a literal `kgrep`.** The curated `Grep`
   definition is gated on `has_backing(["grep"])`. No `grep` tool exists, so it
   is dropped. `"grep"` and `"glob"` sit in `OAUTH_BUILTIN_LOCAL_TOOLS`
   (lib.rs:414) as ghosts. The test
   `oauth_format_tools_drops_builtins_missing_from_registry` enforces the drop
   and asserts `kgrep` is advertised literally.

## Why renaming is not the answer

The name is deliberate branding: the backing crate is `zipadoodlez/kgrep`.
History: `4c4ff04c` removed the native grep/glob/lsp/codesearch tools,
`3f854de3` aliased grep calls to agentgrep, `83b66064` renamed agentgrep to
kgrep. The inbound path was updated carefully. The outbound and discovery path
kept the old names. That asymmetry is the clunk.

## Leverage order, if it is ever fixed

1. One prompt line naming the tool and ruling out `rg`, `grep`, `find`.
   Cheapest, reaches every provider, addresses the bash escape hatch.
2. A description that states capability and status.
3. Remove the stale `grep`/`glob` residue, or rebind the curated `Grep`.
   Caution: the curated schema advertises `output_mode`, `-B`, `-A`, `-C`,
   `head_limit`, `offset`, `multiline`, none of which `KgrepInput` accepts. A
   naive rebind produces failing calls. It needs a schema tracking the real
   input.
4. A name alias is highest leverage and highest blast radius: config enabled
   lists, permissions, ACP, 50 quoted call sites.

## Side finding: LaTeX math

Unrelated to kgrep, found in the same cycle.

- The fork cut removed the old image-based LaTeX path (`152e1250`).
- The math engine was kept and exists: `crates/kcode-render-core/src/math.rs`,
  1234 lines, dependency-free Unicode renderer, `render_inline_latex` and
  `render_display_latex`, plus `normalize_latex_math`. Its 18 tests pass.
- `kcode-tui-markdown` wired it in at `41771c58`.
- The running binary `cd795377` predates that wiring. At that build
  `kcode-tui-markdown/src/lib.rs` had `raw_math_inline_span` / raw
  `raw_math_display_lines`, which print `$math$` and a box headed
  `math (raw)`. That is why math shows as raw source in this session.
- At `80e3644c` the renderer produces proper Unicode math in a
  `math` box.

## Note on method

`rg` is not installed on this machine. Several greps in this cycle failed
silently (`bash: rg: command not found`) and their empty output was briefly
read as "no matches", producing one wrong claim that had to be retracted. Use
`grep -rn`, `kgrep`, or `find` here.
