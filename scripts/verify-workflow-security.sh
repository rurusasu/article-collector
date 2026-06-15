#!/usr/bin/env bash
# Validate GitHub Actions hardening decisions that protect release automation.
set -euo pipefail

CHECKOUT_REF="9f698171ed81b15d1823a05fc7211befd50c8ae0"          # actions/checkout v6.0.3
CACHE_REF="27d5ce7f107fe9357f9df03efb73ab90386fccae"             # actions/cache v5.0.5
APP_TOKEN_REF="bcd2ba49218906704ab6c1aa796996da409d3eb1"         # actions/create-github-app-token v3.2.0
RELEASE_PLZ_REF="e8792575c7f2366cf6ff3ccc33ead9ace5b691c7"       # release-plz/action v0.5.130

failures=0

require_contains() {
  local file="$1"
  local needle="$2"
  local description="$3"

  if file_contains "$file" "$needle"; then
    echo "PASS: $description"
  else
    echo "FAIL: $description" >&2
    echo "  Missing '$needle' in $file" >&2
    failures=$((failures + 1))
  fi
}

require_not_contains() {
  local file="$1"
  local needle="$2"
  local description="$3"

  if file_contains "$file" "$needle"; then
    echo "FAIL: $description" >&2
    echo "  Unexpected '$needle' in $file" >&2
    failures=$((failures + 1))
  else
    echo "PASS: $description"
  fi
}

require_no_workflow_match() {
  local pattern="$1"
  local description="$2"
  local matches=()
  local file line line_no

  for file in .github/workflows/*.yml; do
    [ -f "$file" ] || continue
    line_no=0
    while IFS= read -r line || [ -n "$line" ]; do
      line_no=$((line_no + 1))
      if [[ "$line" =~ $pattern ]]; then
        matches+=("$file:$line_no:$line")
      fi
    done < "$file"
  done

  if [ "${#matches[@]}" -gt 0 ]; then
    echo "FAIL: $description" >&2
    printf '%s\n' "${matches[@]}" >&2
    failures=$((failures + 1))
  else
    echo "PASS: $description"
  fi
}

file_contains() {
  local file="$1"
  local needle="$2"
  local line

  while IFS= read -r line || [ -n "$line" ]; do
    if [[ "$line" == *"$needle"* ]]; then
      return 0
    fi
  done < "$file"

  return 1
}

release=".github/workflows/release.yml"
ci=".github/workflows/ci.yml"
pr_checklist=".github/workflows/pr-checklist.yml"

require_contains "$release" "uses: actions/create-github-app-token@$APP_TOKEN_REF" "release workflow creates a GitHub App token from a pinned action"
require_contains "$release" "uses: release-plz/action@$RELEASE_PLZ_REF" "release-plz action is pinned"
require_contains "$release" "client-id: \${{ secrets.RELEASE_PLEASE_APP_CLIENT_ID }}" "release workflow uses GitHub App client ID secret"
require_contains "$release" "command: release-pr" "release-plz release-pr command is configured"
require_contains "$release" "GITHUB_TOKEN: \${{ steps.release-token.outputs.token }}" "release-plz uses the GitHub App token"
require_contains "$release" "releases_created: \${{ steps.release.outputs.releases_created }}" "release workflow exposes whether a GitHub release was created"
require_contains "$release" "tag_exists: \${{ steps.release.outputs.tag_exists }}" "release workflow exposes whether the Cargo version tag already exists"
require_contains "$release" "tag_name: \${{ steps.release.outputs.tag_name }}" "release workflow exposes the Cargo version tag"
require_contains "$release" "cargo metadata --no-deps --format-version 1" "release workflow derives the tag from Cargo metadata"
require_contains "$release" "github.event_name == 'push' && needs.release.outputs.tag_exists == 'true'" "release-plz PR only runs after an existing release tag"
require_contains "$release" "if [[ \"\$pr_head\" == release-plz-* ]]" "release workflow only auto-releases release-plz PR merges"
require_contains "$release" "if [ \"\$GITHUB_EVENT_NAME\" = \"workflow_dispatch\" ]" "release workflow supports manual release recovery"
require_contains "$release" "gh release create \"\$TAG_NAME\"" "release workflow creates GitHub releases without cargo publish"
require_contains "$release" "GH_TOKEN: \${{ steps.release-token.outputs.token }}" "release asset uploads use the GitHub App token"
require_contains "$release" "permission-contents: write" "release token requests contents write"
require_contains "$release" "permission-pull-requests: read" "release token can inspect the PR that introduced a main commit"
require_contains "$release" "permission-pull-requests: write" "release token requests pull request write"
require_not_contains "$release" "  contents: write" "GITHUB_TOKEN is not granted contents write in release workflow"
require_not_contains "$release" "release-please-action" "release workflow no longer uses release-please"
require_no_workflow_match 'command:[[:space:]]+release[[:space:]]*$' "release workflow does not run release-plz release"
require_not_contains "$release" "permission-workflows:" "release GitHub App token cannot edit workflow files"
require_not_contains "$release" "permission-actions:" "release GitHub App token cannot manage Actions"
require_not_contains "$release" "permission-secrets:" "release GitHub App token cannot manage repository secrets"
require_not_contains "$release" "app-id:" "release workflow avoids deprecated create-github-app-token app-id input"

require_contains "$ci" "permissions:" "CI declares explicit GITHUB_TOKEN permissions"
require_contains "$ci" "release-pr-scope:" "CI verifies release PR file scope"
require_contains "$ci" "if: startsWith(github.head_ref, 'release-plz-')" "CI scopes release PR guard to release-plz branches"
require_contains "$ci" "bash scripts/verify-release-pr-scope.sh \"\$PR_NUMBER\"" "CI runs the release PR scope guard"
require_contains "$pr_checklist" "pull-requests: read" "PR checklist workflow can read PR metadata only"
require_not_contains "$pr_checklist" "pull-requests: write" "PR checklist workflow cannot write PRs"

require_contains ".github/workflows/ci.yml" "uses: actions/checkout@$CHECKOUT_REF" "CI checkout action is SHA-pinned"
require_contains ".github/workflows/pr-checklist.yml" "uses: actions/checkout@$CHECKOUT_REF" "PR checklist checkout action is SHA-pinned"
require_contains ".github/workflows/release.yml" "uses: actions/checkout@$CHECKOUT_REF" "Release checkout action is SHA-pinned"
require_contains ".github/workflows/ci.yml" "uses: actions/cache@$CACHE_REF" "CI cache action is SHA-pinned"
require_contains ".github/workflows/pr-checklist.yml" "uses: actions/cache@$CACHE_REF" "PR checklist cache action is SHA-pinned"
require_contains ".github/workflows/release.yml" "uses: actions/cache@$CACHE_REF" "Release cache action is SHA-pinned"

require_no_workflow_match 'uses: [^ ]+@v[0-9]' "workflows do not use movable major or version tags"
require_no_workflow_match 'dtolnay/rust-toolchain' "Rust toolchain is installed with rustup instead of a third-party action"
require_no_workflow_match 'pull_request_target' "workflows do not use pull_request_target"

if [ "$failures" -gt 0 ]; then
  echo "Workflow security verification failed: $failures issue(s)." >&2
  exit 1
fi

echo "Workflow security verification passed."
