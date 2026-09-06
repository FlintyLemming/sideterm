#!/usr/bin/env bash
# The name used for the release and for the version embedded in the packages.
# A tag build uses the tag itself; everything else falls back to a timestamped
# name derived from the commit being built.
if [[ "${GITHUB_REF:-}" == refs/tags/* ]] ; then
  echo "${GITHUB_REF#refs/tags/}"
else
  git -c "core.abbrev=8" show -s "--format=%cd-%h" "--date=format:%Y%m%d-%H%M%S"
fi
