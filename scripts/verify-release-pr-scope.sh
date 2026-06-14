#!/usr/bin/env bash
# Verify that release-please PRs only contain release metadata changes.
set -euo pipefail

usage() {
  echo "Usage: $0 <PR_NUMBER>"
  exit 1
}

[ $# -eq 1 ] || usage

PR_NUMBER="$1"
if ! [[ "$PR_NUMBER" =~ ^[0-9]+$ ]]; then
  echo "ERROR: Invalid PR number: $PR_NUMBER" >&2
  usage
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "ERROR: gh CLI is required" >&2
  exit 1
fi

allowed='^(\.release-please-manifest\.json|Cargo\.toml|Cargo\.lock)$'
changed_files="$(gh pr diff "$PR_NUMBER" --name-only)"

if [ -z "$changed_files" ]; then
  echo "ERROR: PR #$PR_NUMBER has no changed files" >&2
  exit 1
fi

unexpected=()
while IFS= read -r path; do
  [ -n "$path" ] || continue
  if ! [[ "$path" =~ $allowed ]]; then
    unexpected+=("$path")
  fi
done <<< "$changed_files"

if [ "${#unexpected[@]}" -gt 0 ]; then
  echo "ERROR: release PR contains files outside the release allowlist:" >&2
  printf '  - %s\n' "${unexpected[@]}" >&2
  exit 1
fi

echo "Release PR scope verified."
