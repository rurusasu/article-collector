#!/bin/bash
# Run this after `gh auth login` to configure git user info automatically.
set -e

if ! gh auth status &>/dev/null; then
  echo "Error: Run 'gh auth login' first."
  exit 1
fi

GH_USER=$(gh api user --jq '.login')
GH_NAME=$(gh api user --jq '.name // .login')
GH_EMAIL=$(gh api user/emails --jq '[.[] | select(.primary)] | .[0].email')

cat > ~/.gitconfig.local <<EOF
[user]
    name = ${GH_NAME}
    email = ${GH_EMAIL}

[credential "https://github.com"]
    helper = !/usr/bin/gh auth git-credential
EOF

echo "[setup-git] Done: ${GH_NAME} <${GH_EMAIL}>"
