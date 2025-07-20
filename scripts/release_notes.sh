#!/usr/bin/env bash
# Generate release notes based on conventional commits since last tag.
set -euo pipefail

LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
if [[ -z "$LAST_TAG" ]]; then
  RANGE="HEAD"
else
  RANGE="$LAST_TAG..HEAD"
fi

echo "## Changelog"

git log --pretty=format:'* %s (%h)' --abbrev-commit $RANGE | grep -v '^* Merge' || true 