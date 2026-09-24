# Trimming kcode

Measured 2026-09-24, at commit `a41b1a0f8`.

- Repo pack: **355.45 MB**
- Working tree (no `.git`, no `target`): **27 MB**
- Commits: 9,451

The entire problem is history, not the tree. Cutting 241,527 lines of source
barely moved the pack, because source compresses well and history keeps every
revision forever.

## A. The one lever that matters: history

History weight summed over every revision, by extension:

| ext | total across revisions | what it is |
|---|---|---|
| `.rs` | **1501.7 MB** | every revision of every source file |
| `.gif` | **167.0 MB** | `assets/readme/` demo GIFs (up to 86.5 MB each) |
| `.mp4` | **96.2 MB** | `assets/demos/` demo videos |
| `.lock` | **83.1 MB** | `Cargo.lock`, revised thousands of times |
| `.pcm` | **75.0 MB** | raw audio fixtures |
| `.md` | 14.6 MB | docs, revised |
| `.dSYM` | 6.0 MB | committed iOS/Xcode build artifacts |
| `.png` | 5.5 MB | images |
| `.ttf` | 5.3 MB | fonts |
| `.json` | 5.3 MB | catalogs/caches |
| `.toml` | 4.9 MB | |
| `.o` | 4.8 MB | committed object files |
| `.py` / `.sh` / `.yml` | ~9.5 MB | scripts/CI |
| `.xctest` | 2.4 MB | committed test binaries |
| `.swiftinterface` | 1.4 MB | Xcode intermediates |

Directories that no longer exist in the tree but remain in history (deletion
counts from `--diff-filter=D`): `ios` 4674, `crates` 723, `scripts` 122,
`docs` 65, `telemetry-worker` 55, `sdk` 52, `tests` 34, `src` 27, `assets` 26,
`mockups` 12, `subscription-worker` 7, `figma` 6.

### Two ways to attack it

**A1. Squash to one commit (maximal).** Replaces the 9,451-commit history with a
single initial commit. Drops everything above at once: assets, iOS artifacts,
`.pcm`, `.o`, and 1501 MB of source revisions. Expected result **~30-40 MB**
(basically the working tree plus one snapshot). Cost: no blame, no upstream
ancestry, no `git log`. Fits the "kcode is a product, never merging upstream"
decision.

**A2. Filter paths from history (partial).** Rewrites history but keeps it, so
you keep blame and roughly jcode's shape. Remove `assets/`, the iOS/Xcode
artifacts, `*.pcm`, `*.o`, and the deleted dirs. Still leaves 1501 MB of `.rs`
revisions and 83 MB of `Cargo.lock` revisions, so expect **~120-180 MB**, not
tens. Requires `git-filter-repo` (not installed; `pip install git-filter-repo`).

A1 is strictly more aggressive and is the only path that actually gets you to
"small". A2 is for people who still want history.

## B. Tracked cruft in the working tree (delete outright)

| path | size | why |
|---|---|---|
| `.jcode/semantic-todo-migration-spec.md`, `.jcode/skills/optimization/SKILL.md` | 24 KB | committed project-local jcode state, belongs in `~/.jcode`, not the repo |
| `.claude/` | 8 KB | tool-specific agent config |
| `fork-prompt-session-rich-summary.json` | 52 KB | stray working artifact |
| `fork-prompt-session-todos-and-intents.json` | 12 KB | stray working artifact |
| `indian-man-portrait.svg` | 4 KB | unrelated image at repo root |
| `score_shard.py` | 4 KB | unrelated script at repo root |
| `scripts/mermaid_fit_probe.py` | 4 KB | dead residue from the feature cut |
| `graphify-out/` | 236 KB | untracked tool output; add to `.gitignore` |

## C. Docs and metadata (trim, do not delete wholesale)

- `docs/` 60 files, 1.2 MB, of which `docs/images` 636 KB (6 files) and
  `docs/proposals`, `docs/audits`, `docs/plans` ~92 KB.
- `changelog/` 324 KB.
- Root docs: `README.md` (rewrite, requested), `RELEASING.md`, `OAUTH.md`,
  `CLAUDE.md`, `AGENTS.md`, `CONTRIBUTING.md`. Several describe jcode the
  product; each needs a keep/rewrite/delete call.

## D. Not trim-able without a rewrite

- `Cargo.lock` revisions (83 MB across history) are only reduced by A1/A2.
- The 62 `jcode-*` crate names (Phase 3 of the rename) are source churn, not
  size.

## Recommended order

1. Delete section B, add `graphify-out/` to `.gitignore`.
2. Rewrite `README.md` for kcode.
3. Pick A1 or A2 and rewrite history, then `git gc`, then force-push `main`.
4. Trim section C as a follow-up pass.

Nothing outside the `kcode` remote is affected by any of this.
