#!/usr/bin/env bash
# wait-ci.sh — PRのCI完了を待ち、結果をJSON形式で返す
# Usage: bash scripts/wait-ci.sh
#
# PRが存在しない場合は即座にスキップする。
# CI完了まで最大5分ポーリングし、結果を返す。
set -euo pipefail

POLL_INTERVAL=15
MAX_WAIT=300
INITIAL_DELAY=10

# 現在のブランチからPR番号を取得
PR_NUMBER=$(gh pr view --json number -q '.number' 2>/dev/null) || true

if [ -z "$PR_NUMBER" ]; then
  echo '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"Push completed. No open PR found for current branch."}}'
  exit 0
fi

# GitHub Actions がチェックを登録するまで待機
sleep "$INITIAL_DELAY"
ELAPSED="$INITIAL_DELAY"

escape_json_string() {
  printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\t/\\t/g' | tr -d '\r' | tr '\n' ' '
}

while [ "$ELAPSED" -lt "$MAX_WAIT" ]; do
  # gh pr checks を実行（stderr は分離）
  if ! CHECKS_OUTPUT=$(gh pr checks "$PR_NUMBER" 2>/dev/null); then
    EXIT_CODE=$?
    # Exit code 8 = checks pending (gh CLI convention)
    if [ "$EXIT_CODE" -eq 8 ]; then
      sleep "$POLL_INTERVAL"
      ELAPSED=$((ELAPSED + POLL_INTERVAL))
      continue
    fi
    # checks が失敗を含む場合も exit code != 0
    # fall through to check output below
  fi

  # 出力が空またはチェック未登録の場合は待機
  if [ -z "$CHECKS_OUTPUT" ]; then
    sleep "$POLL_INTERVAL"
    ELAPSED=$((ELAPSED + POLL_INTERVAL))
    continue
  fi

  # まだ pending があるか確認
  if echo "$CHECKS_OUTPUT" | grep -qiE "pending|in_progress"; then
    sleep "$POLL_INTERVAL"
    ELAPSED=$((ELAPSED + POLL_INTERVAL))
    continue
  fi

  # 全チェック完了 — fail があるか確認
  if echo "$CHECKS_OUTPUT" | grep -qi "	fail	"; then
    FAILED_SUMMARY=$(echo "$CHECKS_OUTPUT" | grep -i "	fail	" | head -5)

    # 最新の失敗した run のログを取得
    FAILED_RUN_ID=$(gh run list --branch "$(git branch --show-current)" --status failure --limit 1 --json databaseId -q '.[0].databaseId' 2>/dev/null) || true
    FAILED_LOG=""
    if [ -n "$FAILED_RUN_ID" ]; then
      FAILED_LOG=$(gh run view "$FAILED_RUN_ID" --log-failed 2>/dev/null | tail -30) || true
    fi

    ESCAPED_SUMMARY=$(escape_json_string "$FAILED_SUMMARY")
    ESCAPED_LOG=$(escape_json_string "$FAILED_LOG")

    echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PostToolUse\",\"additionalContext\":\"CI FAILED for PR #${PR_NUMBER}. Failed checks: ${ESCAPED_SUMMARY}. Logs: ${ESCAPED_LOG}. You MUST fix these CI failures before proceeding.\"}}"
    exit 0
  fi

  # 全チェック pass
  PASS_COUNT=$(echo "$CHECKS_OUTPUT" | grep -c "	pass	" || true)
  if [ "$PASS_COUNT" -eq 0 ]; then
    # pass が検出できない場合は待機（まだ登録中の可能性）
    sleep "$POLL_INTERVAL"
    ELAPSED=$((ELAPSED + POLL_INTERVAL))
    continue
  fi

  echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PostToolUse\",\"additionalContext\":\"CI passed for PR #${PR_NUMBER}. All ${PASS_COUNT} checks green. If a PR exists, execute the review process defined in docs/pr/README.md. Loop until all issues are resolved.\"}}"
  exit 0
done

# タイムアウト
echo "{\"hookSpecificOutput\":{\"hookEventName\":\"PostToolUse\",\"additionalContext\":\"CI still running for PR #${PR_NUMBER} after ${MAX_WAIT}s timeout. Run 'gh pr checks ${PR_NUMBER} --watch' to continue monitoring.\"}}"
exit 0
