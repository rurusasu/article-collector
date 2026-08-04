#!/bin/sh
set -eu

repository_url="https://github.com/rurusasu/article-collector"

error() {
  printf 'error: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command_name=$1
  command -v "$command_name" >/dev/null 2>&1 ||
    error "required command not found: $command_name"
}

for command_name in curl uname mkdir mktemp mv chmod rm; do
  require_command "$command_name"
done

if [ -z "${HOME:-}" ]; then
  error "HOME is not set"
fi

os=$(uname -s)
architecture=$(uname -m)

case "$os/$architecture" in
  Darwin/arm64 | Darwin/aarch64)
    asset="article-collector-macos-arm64"
    ;;
  Darwin/x86_64 | Darwin/amd64)
    asset="article-collector-macos-amd64"
    ;;
  Linux/aarch64 | Linux/arm64)
    asset="article-collector-linux-arm64"
    ;;
  Linux/x86_64 | Linux/amd64)
    asset="article-collector-linux-amd64"
    ;;
  *)
    error "unsupported platform: $os/$architecture (supported: macOS/Linux amd64/arm64)"
    ;;
esac

install_dir="$HOME/.local/bin"
install_path="$install_dir/article-collector"
download_url="$repository_url/releases/latest/download/$asset"
temp_file=$(mktemp "${TMPDIR:-/tmp}/article-collector.XXXXXX")

cleanup() {
  rm -f "$temp_file"
}
trap cleanup 0
trap 'exit 1' 1 2 3 15

mkdir -p "$install_dir"
printf 'Downloading %s...\n' "$asset"
curl -fsSL "$download_url" -o "$temp_file"
chmod 0755 "$temp_file"
mv -f "$temp_file" "$install_path"

printf 'Installed article-collector to %s\n' "$install_path"
case ":${PATH:-}:" in
  *:"$install_dir":*)
    ;;
  *)
    printf "Add it to PATH if needed: export PATH=\"%s:\$PATH\"\n" "$install_dir"
    ;;
esac
