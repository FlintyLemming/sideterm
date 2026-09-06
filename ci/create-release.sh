#!/usr/bin/env bash
set -x
name="$1"

notes=$(cat <<EOT
SideTerm $name

Packages for Windows, macOS and Linux are attached below.
EOT
)

gh release view "$name" || gh release create --prerelease --notes "$notes" --title "$name" "$name"
