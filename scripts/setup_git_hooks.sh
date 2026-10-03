#!/usr/bin/env bash
# Point git at the hooks committed in .githooks/. `core.hooksPath` is local git
# config and does not travel with a clone, so run this once per checkout.
#
# The committed hooks no-op when graphify is not installed. `graphify-out/` is an
# untracked local cache, so build it once per clone with `graphify update .`.
set -euo pipefail
cd "$(dirname "$0")/.."

git config core.hooksPath .githooks
echo "core.hooksPath -> .githooks (pre-push gate; post-commit/post-checkout graph refresh)"
