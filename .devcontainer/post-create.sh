#!/bin/bash
set -e

echo "[devcontainer] Installing additional tools..."

# go-task
TASK_VERSION="3.40.1"
curl -sSL "https://github.com/go-task/task/releases/download/v${TASK_VERSION}/task_linux_amd64.tar.gz" \
  | sudo tar -xz -C /usr/local/bin task

# Python packages
pip3 install --user youtube-transcript-api

# Create working directory for scripts
mkdir -p /tmp/collect

# AI coding assistants
npm install -g @anthropic-ai/claude-code @openai/codex

# Rust: fetch dependencies
cargo fetch

# Git config: include repo config instead of symlinking
# Symlinking causes VS Code's credential helper injection to modify the repo file directly.
# Using [include] keeps the repo file read-only while still applying its settings.
REPO_ROOT="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
git config --global include.path "${REPO_ROOT}/.github/.gitconfig"

echo ""
echo "[devcontainer] Run the following to complete git setup:"
echo "  gh auth login"
echo "  gh auth refresh -h github.com -s user"
echo "  .github/setup-git.sh"
echo ""
echo "[devcontainer] Setup complete"
echo "  rust:       $(rustc --version)"
echo "  cargo:      $(cargo --version)"
echo "  task:       $(task --version)"
echo "  jq:         $(jq --version)"
echo "  gh:         $(gh --version | head -1)"
echo "  python3:    $(python3 --version)"
echo "  shellcheck: $(shellcheck --version | head -2 | tail -1)"
echo "  bats:       $(bats --version)"
echo "  claude:     $(claude --version 2>&1 || echo 'NOT FOUND')"
echo "  codex:      $(codex --version 2>&1 || echo 'NOT FOUND')"
