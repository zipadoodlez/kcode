#!/usr/bin/env bash
# Point git at the hooks committed in .githooks/. `core.hooksPath` is local git
# config and does not travel with a clone, so run this once per checkout.
#
# The committed hooks no-op when graphify is not installed. `graphify hook
# install` additionally registers graphify's graph.json union-merge driver
# (declared in .gitattributes); run it too if you expect graph.json conflicts.
set -euo pipefail
cd "$(dirname "$0")/.."

git config core.hooksPath .githooks
echo "core.hooksPath -> .githooks (post-commit/post-checkout graph refresh)"
