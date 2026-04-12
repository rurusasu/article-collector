#!/usr/bin/env bash
# verify-pr.sh — PR チェックリストを自動検証する
# Usage: bash scripts/verify-pr.sh <PR_NUMBER>
#        bash scripts/verify-pr.sh --local
#
# 依存: gh, bash 4+
# jq 不要 — 純粋な bash で動作
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RESULT_FILE="/tmp/verify-pr-result.json"

# Load rules
source "$SCRIPT_DIR/verify-rules.sh"

# Colors (if terminal supports)
if [ -t 1 ]; then
  GREEN='\033[0;32m'
  RED='\033[0;31m'
  YELLOW='\033[0;33m'
  BOLD='\033[1m'
  RESET='\033[0m'
else
  GREEN='' RED='' YELLOW='' BOLD='' RESET=''
fi

usage() {
  echo "Usage: $0 <PR_NUMBER> | --local"
  echo "  <PR_NUMBER>  Verify checklist items from PR description"
  echo "  --local      Run all rules without a PR"
  exit 1
}

[ $# -lt 1 ] && usage

MODE="$1"
PASSED=0
FAILED=0
SKIPPED=0
RESULT_ITEMS=""

escape_json() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\t/\\t/g'
}

run_rule() {
  local description="$1"
  local command="$2"
  local item_text="$3"
  local escaped_item
  escaped_item=$(escape_json "$item_text")

  printf "  %-55s " "${item_text:0:55}"
  # Safety: $command comes only from the checked-in verify-rules.sh file,
  # never from PR body or other user input. eval is safe here.
  if eval "$command" > /dev/null 2>&1; then
    echo -e "${GREEN}[PASS]${RESET} $description"
    RESULT_ITEMS="${RESULT_ITEMS}{\"item\":\"${escaped_item}\",\"status\":\"pass\"},"
    PASSED=$((PASSED + 1))
    return 0
  else
    echo -e "${RED}[FAIL]${RESET} $description"
    RESULT_ITEMS="${RESULT_ITEMS}{\"item\":\"${escaped_item}\",\"status\":\"fail\"},"
    FAILED=$((FAILED + 1))
    return 1
  fi
}

match_and_run() {
  local item_text="$1"
  local matched=0

  for rule in "${VERIFY_RULES[@]}"; do
    local pattern command description
    pattern=$(echo "$rule" | awk -F':::' '{print $1}')
    command=$(echo "$rule" | awk -F':::' '{print $2}')
    description=$(echo "$rule" | awk -F':::' '{print $3}')

    if echo "$item_text" | grep -qiE "$pattern"; then
      run_rule "$description" "$command" "$item_text" || true
      matched=1
      break
    fi
  done

  if [ "$matched" -eq 0 ]; then
    local escaped_item
    escaped_item=$(escape_json "$item_text")
    printf "  %-55s " "${item_text:0:55}"
    echo -e "${YELLOW}[SKIP]${RESET} No matching rule (manual verification required)"
    RESULT_ITEMS="${RESULT_ITEMS}{\"item\":\"${escaped_item}\",\"status\":\"skip\"},"
    SKIPPED=$((SKIPPED + 1))
  fi
}

echo ""
echo -e "${BOLD}=== PR Checklist Verification ===${RESET}"
echo ""

if [ "$MODE" = "--local" ]; then
  echo "Mode: local (running all rules)"
  echo ""

  for rule in "${VERIFY_RULES[@]}"; do
    command=$(echo "$rule" | awk -F':::' '{print $2}')
    description=$(echo "$rule" | awk -F':::' '{print $3}')
    run_rule "$description" "$command" "$description" || true
  done
else
  PR_NUMBER="$MODE"

  if ! [[ "$PR_NUMBER" =~ ^[0-9]+$ ]]; then
    echo "ERROR: Invalid PR number: $PR_NUMBER"
    usage
  fi

  # Extract PR body
  PR_BODY=$(gh pr view "$PR_NUMBER" --json body -q '.body' 2>/dev/null) || {
    echo "ERROR: Failed to fetch PR #$PR_NUMBER"
    exit 1
  }

  echo "PR #$PR_NUMBER"
  echo ""

  # Parse checklist items (both checked and unchecked)
  ITEMS=()
  while IFS= read -r line; do
    [ -n "$line" ] && ITEMS+=("$line")
  done <<< "$(echo "$PR_BODY" | tr -d '\r' | sed -n 's/^- \[[ xX]\] \(.*\)/\1/p')"

  if [ ${#ITEMS[@]} -eq 0 ]; then
    echo "No checklist items found in PR description."
    echo '{"status":"pass","passed":0,"failed":0,"skipped":0,"timestamp":"'"$(date -u +%Y-%m-%dT%H:%M:%SZ)"'","items":[]}' > "$RESULT_FILE"
    exit 0
  fi

  echo "Found ${#ITEMS[@]} checklist item(s):"
  echo ""

  for item in "${ITEMS[@]}"; do
    match_and_run "$item"
  done
fi

# Summary
echo ""
echo -e "${BOLD}--- Result ---${RESET}"
echo -e "  Passed:  ${GREEN}${PASSED}${RESET}"
echo -e "  Failed:  ${RED}${FAILED}${RESET}"
echo -e "  Skipped: ${YELLOW}${SKIPPED}${RESET}"
echo ""

# Write result file (manual JSON — no jq needed)
if [ "$FAILED" -gt 0 ]; then
  STATUS="fail"
else
  STATUS="pass"
fi

TIMESTAMP=$(date -u +%Y-%m-%dT%H:%M:%SZ)
# Remove trailing comma from items
RESULT_ITEMS="${RESULT_ITEMS%,}"

cat > "$RESULT_FILE" <<EOF
{"status":"${STATUS}","passed":${PASSED},"failed":${FAILED},"skipped":${SKIPPED},"timestamp":"${TIMESTAMP}","items":[${RESULT_ITEMS}]}
EOF

if [ "$FAILED" -gt 0 ]; then
  echo -e "${RED}VERIFICATION FAILED${RESET}"
  exit 1
else
  echo -e "${GREEN}VERIFICATION PASSED${RESET}"
  exit 0
fi
