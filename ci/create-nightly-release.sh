#!/usr/bin/env bash
# Idempotently create the rolling "nightly" pre-release that the continuous
# builds upload their assets to. Safe to run from several jobs at once: the
# loser of a create race simply sees the release on its next attempt.
set -x
set -e

gh release view nightly >/dev/null 2>&1 && exit 0

notes=$(cat <<'EOT'
Automated build of the latest commit on `main`.

These assets are replaced every time the nightly workflows run.
EOT
)

gh release create --prerelease --target main --title "Nightly" --notes "$notes" nightly
