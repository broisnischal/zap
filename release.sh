#!/usr/bin/env bash
set -euo pipefail

# Simple release helper for zap
# Usage:
#   ./release.sh 0.1.1
#
# This will:
#   1. Bump version in Cargo.toml and src/main.rs
#   2. Commit the version bump
#   3. Create an annotated git tag v<version>
#   4. Push commit and tag to origin (triggering CI/CD)

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT_DIR"

if [ $# -ne 1 ]; then
  echo "Usage: $0 NEW_VERSION (e.g. 0.1.1)" >&2
  exit 1
fi

NEW_VERSION="$1"

if ! [[ "$NEW_VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Error: version must be semantic (e.g. 0.1.1)" >&2
  exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Error: working tree is dirty. Commit or stash changes before releasing." >&2
  exit 1
fi

if [ ! -f Cargo.toml ]; then
  echo "Error: Cargo.toml not found in $(pwd)" >&2
  exit 1
fi

OLD_VERSION="$(grep '^version = "' Cargo.toml | head -1 | sed -E 's/version = "(.*)"/\1/')"

echo "Releasing zap $OLD_VERSION -> $NEW_VERSION"

# Update Cargo.toml
sed -i "s/^version = \"$OLD_VERSION\"/version = \"$NEW_VERSION\"/" Cargo.toml

# Update clap version in main.rs (command(version = "..."))
if grep -q "command(version = \"$OLD_VERSION\")" src/main.rs; then
  sed -i "s/command(version = \"$OLD_VERSION\")/command(version = \"$NEW_VERSION\")/" src/main.rs
else
  echo "Warning: could not find command(version = \"$OLD_VERSION\") in src/main.rs; skipping" >&2
fi

git add Cargo.toml src/main.rs || true

# Ask user for a short description of the changeset
echo
echo "Describe what changed in this release."
read -rp "Short release summary (for commit/tag): " RELEASE_SUMMARY

if git diff --cached --quiet; then
  echo "No version changes detected; aborting." >&2
  exit 1
fi

if [ -z "${RELEASE_SUMMARY:-}" ]; then
  RELEASE_SUMMARY="no description provided"
fi

git commit -m "chore: release v$NEW_VERSION - $RELEASE_SUMMARY"

TAG="v$NEW_VERSION"

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Error: tag $TAG already exists." >&2
  exit 1
fi

git tag -a "$TAG" -m "$TAG - $RELEASE_SUMMARY"

echo "Pushing commit and tag to origin..."
git push origin HEAD
git push origin "$TAG"

echo "Release $TAG pushed. CI/CD pipeline should pick this up."


