# Render-Core Parity: Acceptance Criteria, Thresholds, and Statistical Reporting

Status: adopted for the `jcode-render-core` -> TUI switchover.
Harness: `crates/jcode-tui-markdown/src/render_core_adapter_tests.rs`
(differential tests comparing `render_markdown_via_core*` against the legacy
`render_markdown*` pipeline).

## 1. Parity levels (what is measured)

Each level is a distinct, machine-checkable comparison function. A level only
counts as covered when a test asserts it directly.

| Level | Definition | Comparator | Tests |
|-------|------------|------------|-------|
| L1: Content parity | Whitespace-collapsed visible text is identical | `flattened()` | `parity_*`, `fuzz_visible_text_parity`, `fuzz_random_documents_parity` |
| L2: Line-structure parity | Per-line trimmed non-blank visible text is identical (catches line-break divergence) | `nonblank_texts()` | `fuzz_random_documents_line_structure` |
| L3: Wrapped-layout parity | L2 comparison after production wrapping at widths {20, 40, 80} | `nonblank_texts()` on wrapped output | `fuzz_random_documents_wrapped_parity` |
| L4: Style invariants | Targeted styling equivalence: math fg spans identical, bold carries BOLD, inline code carries bg fill, headings colored by level, display math framed | span-level predicates | `probe_math_divergence`, `core_marks_bold_and_code_styling`, `core_renders_display_math_frame`, etc. |

Deliberately out of scope (documented divergence, not failure): blank-line
padding counts, decorative marker glyph choices, and exact `Style` equality on
non-invariant spans. Any new intentional divergence must be listed here.

## 2. Acceptance thresholds

All differential tests are **zero-tolerance**: the acceptance criterion is
`mismatches == 0` for every level at every tier. There is no "acceptable
mismatch rate"; the statistics below quantify what a passing run *proves*
about the residual mismatch probability, not what we tolerate.

| Tier | Iterations per fuzz suite | Gate | Residual mismatch rate bound (95%, rule of three: p < 3/N) |
|------|---------------------------|------|--------------------------------------------------|
| CI (default) | 5000 (L1, L2), 3000×3 widths = 9000 renders (L3) | must pass on every PR touching `jcode-tui-markdown`, `jcode-render-core` | p < 6.0e-4 per generated document (L1/L2); p < 1.0e-3 per doc per width (L3) |
| Pre-switchover deep run | `JCODE_MD_FUZZ_ITERS=100000` | must pass once, on the exact commit proposed for switchover | p < 3.0e-5 per generated document |
| Nightly (optional soak) | `JCODE_MD_FUZZ_ITERS=25000`, rotating `JCODE_MD_FUZZ_SEED` (e.g. epoch-day) | failures file an issue with repro seed | accumulates coverage across seeds over time |

Rationale for the bound: with N i.i.d. generated documents and 0 observed
mismatches, the one-sided 95% upper confidence bound on the mismatch
probability is `1 - 0.05^(1/N) ≈ 3/N` (the "rule of three").

Fixed-corpus criteria (non-statistical, exhaustive):

- Every entry in the `fuzz_visible_text_parity` corpus (currently 43 cases)
  passes L1. Adding a construct to the generator **requires** adding at least
  one fixed-corpus case for it, so regressions localize.
- Every `parity_*` unit case passes L1. Every L4 invariant test passes.

Switchover gate (all required):

1. CI tier green on the switchover commit.
2. Deep run (100k iters) green for L1, L2, and L3.
3. Deep run repeated with a second seed (any value differing from the
   defaults) green, to reduce seed-specific blind spots.
4. Generator coverage checklist (section 4) has no unchecked construct that
   the legacy renderer supports.

## 3. Statistical reporting requirements

Every fuzz test already implements, and must preserve, this reporting
contract on failure:

- **Reproducibility:** each failure reports the iteration index `i`; the
  per-iteration RNG is derived as
  `seed = base_seed + i * 0x100000001B3`, so any single failure is
  reproducible with `JCODE_MD_FUZZ_SEED=<base_seed> JCODE_MD_FUZZ_ITERS=<i+1>`
  (or by re-deriving the single seed). `base_seed` defaults are fixed
  constants per suite and overridable via `JCODE_MD_FUZZ_SEED`.
- **Bounded failure dump:** collect up to 5 failing cases before aborting the
  loop, then report all of them (input document, core output, legacy output)
  in one assertion message. Never fail on only the first case; multiple
  examples are needed to classify a divergence.
- **Full-input echo:** the raw markdown input is printed verbatim so a
  failing case can be promoted directly into the fixed corpus.

Required additions when reporting results for a switchover decision (manual
or scripted summary, e.g. in the PR description):

- `iters`, `base_seed`, suite name, and pass/fail per level (L1-L4).
- The implied 95% upper bound `3/N` for each passed fuzz suite.
- For any failure found during development: a one-line classification
  (parser divergence, adapter styling, wrap divergence, generator artifact)
  and the corpus case added to pin the fix.

## 4. Generator coverage checklist

The statistical bound only covers the generator's distribution. The
generator (`gen_block`/`gen_inline`) must cover, and the checklist is audited
whenever a construct is added to either renderer:

- [x] headings (1-3), paragraphs, hard/soft breaks
- [x] bold, italic, strikethrough, inline code, links
- [x] inline math, display math, currency-dollar disambiguation
- [x] ordered/unordered/nested/task lists, definition lists
- [x] blockquotes (nested, multiline), thematic breaks
- [x] fenced code blocks (with/without language), tables (1-3 cols)
- [x] footnotes, CJK/emoji text
- [ ] setext headings (fixed corpus only, not in generator)
- [ ] images, autolinks, HTML fragments (fixed corpus only)
- [ ] reference-style links, indented code blocks (uncovered)

Unchecked items must either be added to the generator or explicitly waived in
the switchover PR with a fixed-corpus case demonstrating parity.

## 5. Running

```sh
# CI-equivalent (all levels, default iterations)
cargo test -p jcode-tui-markdown --lib render_core_adapter

# Deep run (switchover gate; ~80s on an XPS 13)
JCODE_MD_FUZZ_ITERS=100000 \
  cargo test -p jcode-tui-markdown --lib render_core_adapter::tests::fuzz

# Alternate-seed confirmation
JCODE_MD_FUZZ_ITERS=100000 JCODE_MD_FUZZ_SEED=20260713 \
  cargo test -p jcode-tui-markdown --lib render_core_adapter::tests::fuzz
```
