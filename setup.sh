#!/bin/bash
set -e

# Claude Code on the Web - setup script
# Runs before each new session on Anthropic's cloud VM.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR"

cd "$REPO_ROOT"
echo "[setup] repo root: $REPO_ROOT"

# --- Install dependencies ---

# jq (JSON processor)
if ! command -v jq &>/dev/null; then
  echo "[setup] installing jq..."
  apt-get update -qq && apt-get install -y -qq jq
fi

# bats (Bash testing framework)
if ! command -v bats &>/dev/null; then
  echo "[setup] installing bats..."
  apt-get update -qq && apt-get install -y -qq bats
fi

# shellcheck (shell script linter)
if ! command -v shellcheck &>/dev/null; then
  echo "[setup] installing shellcheck..."
  apt-get update -qq && apt-get install -y -qq shellcheck
fi

# go-task (task runner)
if ! command -v task &>/dev/null; then
  echo "[setup] installing go-task..."
  sh -c "$(curl --location https://taskfile.dev/install.sh)" -- -d -b /usr/local/bin
fi

# Python3 (should be pre-installed, but verify)
if ! command -v python3 &>/dev/null; then
  echo "[setup] installing python3..."
  apt-get update -qq && apt-get install -y -qq python3
fi

# Create /tmp/collect directory used by scripts
mkdir -p /tmp/collect

echo "[setup] verifying installed tools:"
echo "  jq:         $(jq --version 2>&1 || echo 'NOT FOUND')"
echo "  bats:       $(bats --version 2>&1 || echo 'NOT FOUND')"
echo "  shellcheck: $(shellcheck --version 2>&1 | head -2 | tail -1 || echo 'NOT FOUND')"
echo "  task:       $(task --version 2>&1 || echo 'NOT FOUND')"
echo "  python3:    $(python3 --version 2>&1 || echo 'NOT FOUND')"
echo "  gh:         $(gh --version 2>&1 | head -1 || echo 'NOT FOUND')"
echo "  curl:       $(curl --version 2>&1 | head -1 || echo 'NOT FOUND')"
echo "  git:        $(git --version 2>&1 || echo 'NOT FOUND')"

echo "[setup] done"
