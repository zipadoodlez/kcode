# Todo rework: A/B probe

How to build the pre-change and post-change `kcode` side by side and compare what
the enforcement tier was actually buying. Written for the todo enforcement
removal (2026-09-30), and the recipe works for any behavioral A/B on this tree.
Phase 1 has landed, so the pre-change side now comes from the sha you choose,
not from a checkout that still has the tier. The comparison was not run before
the landing, and it is still worth running: whether the nudge bought verification
is what the result every close has to carry, in `plans/task-flow.md`, has to
replace.

## What is being compared

The tier is one thing: a turn-end nudge that keeps the model working while todos
are open. The question the comparison answers is whether that nudge buys
**verification** (the model names a check and reports its result) or only
**turns** (the model continues but still does not verify).

That question needs both builds, and after step 3 the pre-change state survives
only in git history, so the `before` side is a worktree at a sha before it.

## Route A: two cargo builds (recommended)

No pacman, nothing installed, nothing shared with your live session.

```bash
cd /home/khaled/kcode
sha=$(git rev-parse --short HEAD)      # write this down; both sides get rebuilt from it

# one worktree per side, so neither disturbs the other
git worktree add /tmp/kcode-before <sha-of-the-pre-change-build>
git worktree add /tmp/kcode-after  <sha-of-the-post-change-build>

for side in before after; do
  (cd /tmp/kcode-$side && cargo build --profile selfdev -p kcode --bin kcode)
  cp /tmp/kcode-$side/target/selfdev/kcode /tmp/kcode-$side-bin
done
```

`--profile selfdev` keeps the build short; the behavior under test does not
depend on the profile. Two worktrees cost two `target/` directories, several GB
mostly in the incremental cache. `CARGO_TARGET_DIR=/tmp/kcode-target-$side` keeps
them off your main tree if disk is tight.

Run each binary against its own socket and its own home, so your installed daemon
and your real `~/.kcode` are untouched:

```bash
KCODE_HOME=/tmp/kcode-home-before \
KCODE_AUTO_CONTINUE=1 \
  /tmp/kcode-before-bin run --socket /run/user/$UID/kcode-ab-before.sock '<task>'
```

Repeat with `after` and `kcode-ab-after.sock`. Arming matters: `features.auto_continue`
defaults to `false`, so without the env var you are measuring a disabled feature.
Logs land in `$KCODE_HOME/logs/`; sessions and todo files land in
`$KCODE_HOME`, which is why the two sides need separate homes.

## Route B: PKGBUILD, if you want packaged artifacts

The PKGBUILD clones the remote default branch
(`source=("$pkgname::git+$url.git")`), so it will not build your local branch as
written. Point it at your clone and pin the commit:

```diff
-source=("$pkgname::git+$url.git")
+source=("kcode-git::git+file:///home/khaled/kcode#commit=<sha>")
```

`makepkg -f` for each sha. `pkgver()` derives the version from the clone, so the
two packages get different `pkgver` strings and `pacman -Q kcode-git` tells you
which one is installed.

Both packages are named `kcode-git` and both `conflicts=(kcode)`, so you cannot
have them installed at once. Install one, test, install the other, test, and
restart the daemon between them, because the daemon is the installed binary and
will keep serving the old version until it re-execs (the debug `reload` command
does that). This is why route A is the better tool for a comparison and the
PKGBUILD is the better tool for shipping.

## The task

One that forces both a search and a check, so a run that skips verification is
visible:

> Find the function that builds the auto-poke message in this repository, then
> run a command that proves which file and line it is defined at, and paste its
> output.

## What to record, per side

1. Did the final message name a concrete check and its actual result? Quote the
   sentence, or record that there was none. This is the whole point of the probe.
2. How many turns the nudge bought. The client no longer emits a per-continuation
   decision line, so read the count off the session transcript for the run and
   subtract one for the initial prompt. (The server's continuation loop is the
   nudge now; it is `continue_with_next_row` in
   `kcode-app-core/src/server/live_turn.rs`.)
3. Total turns in the run.

## How to read the result

- Both sides name a check: the tier was buying turns, not verification. Expected,
  and it is the case for deleting it.
- Before names a check, after does not: the tier was load-bearing. Fix with the
  one line the doc's Validation section already names: *for each goal, name the
  check that proves it is done and report its actual result.*
- Neither side names a check: the tier was not working anyway, so the after build
  is not worse, it is cheaper.

## Cleanup

```bash
git worktree remove /tmp/kcode-before
git worktree remove /tmp/kcode-after
rm -rf /tmp/kcode-home-before /tmp/kcode-home-after /tmp/kcode-*-bin
```
