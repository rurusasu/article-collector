#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
original_path=$PATH
test_root=$(mktemp -d "${TMPDIR:-/tmp}/article-collector-install-test.XXXXXX")
mock_bin="$test_root/bin"
mkdir -p "$mock_bin"

cleanup() {
  rm -rf "$test_root"
}
trap cleanup EXIT

cat > "$mock_bin/uname" <<'EOF'
#!/bin/sh
set -eu

case "$1" in
  -s) printf '%s\n' "$TEST_UNAME_S" ;;
  -m) printf '%s\n' "$TEST_UNAME_M" ;;
  *) exit 1 ;;
esac
EOF

cat > "$mock_bin/curl" <<'EOF'
#!/bin/sh
set -eu

output_path=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output_path=$2
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url=$1
      shift
      ;;
  esac
done

printf '%s\n' "$url" > "$FAKE_CURL_URL_LOG"
if [ "${FAKE_CURL_FAIL:-0}" = 1 ]; then
  exit 22
fi

printf 'fake-binary\n' > "$output_path"
EOF

chmod +x "$mock_bin/uname" "$mock_bin/curl"

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

assert_file_content() {
  expected=$1
  path=$2
  message=$3
  actual=$(<"$path")
  [ "$actual" = "$expected" ] || fail "$message (expected '$expected', got '$actual')"
}

run_install() {
  TEST_UNAME_S=$1 \
    TEST_UNAME_M=$2 \
    FAKE_CURL_FAIL=${3:-0} \
    FAKE_CURL_URL_LOG="$CASE_DIR/curl-url.log" \
    HOME="$CASE_DIR/home" \
    PATH="$mock_bin:$original_path" \
    sh "$repo_root/install.sh" >"$CASE_DIR/stdout.log" 2>"$CASE_DIR/stderr.log"
}

new_case() {
  CASE_DIR="$test_root/$1"
  mkdir -p "$CASE_DIR"
}

test_darwin_arm64_selects_release_asset() {
  new_case darwin-arm64
  run_install Darwin arm64
  assert_file_content fake-binary "$CASE_DIR/home/.local/bin/article-collector" \
    'Darwin arm64 binary was not installed'
  assert_file_content \
    'https://github.com/rurusasu/article-collector/releases/latest/download/article-collector-macos-arm64' \
    "$CASE_DIR/curl-url.log" \
    'Darwin arm64 release asset was not selected'
}

test_linux_aarch64_selects_release_asset() {
  new_case linux-aarch64
  run_install Linux aarch64
  assert_file_content fake-binary "$CASE_DIR/home/.local/bin/article-collector" \
    'Linux aarch64 binary was not installed'
  assert_file_content \
    'https://github.com/rurusasu/article-collector/releases/latest/download/article-collector-linux-arm64' \
    "$CASE_DIR/curl-url.log" \
    'Linux aarch64 release asset was not selected'
}

test_unsupported_platform_fails_before_download() {
  new_case freebsd
  if run_install FreeBSD x86_64; then
    fail 'unsupported platform unexpectedly succeeded'
  fi
  [ ! -f "$CASE_DIR/curl-url.log" ] || fail 'curl was called for an unsupported platform'
}

test_download_failure_preserves_existing_binary() {
  new_case download-failure
  mkdir -p "$CASE_DIR/home/.local/bin"
  printf 'existing-binary\n' > "$CASE_DIR/home/.local/bin/article-collector"
  if run_install Darwin arm64 1; then
    fail 'download failure unexpectedly succeeded'
  fi
  assert_file_content existing-binary "$CASE_DIR/home/.local/bin/article-collector" \
    'existing binary changed after download failure'
}

test_darwin_arm64_selects_release_asset
test_linux_aarch64_selects_release_asset
test_unsupported_platform_fails_before_download
test_download_failure_preserves_existing_binary
printf 'PASS: install.sh tests\n'
