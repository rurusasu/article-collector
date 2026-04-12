# Pull Request Guide

## Overview

Defines the process from PR creation to merge.
Loop all review steps until every issue is resolved to ensure quality.

## PR Creation

### Template

```markdown
## Summary
- Bullet-point summary of changes

## Changes
Organized by category

## Linear Issue
Link to the relevant Linear issue

## Test plan
- [ ] List items to verify locally
```

### Creation Command

```bash
gh pr create --title "feat: title" --body "..." --base main
```

## PR Review Process

After PR creation, execute the following steps **in a loop until all issues are resolved**.

```
+---------------------------------------------+
|  Step 1: PR Review                          |
|  Step 2: Security Guidance                  |
|  Step 3: Checklist Verification -> PR Update|
|  Step 4: PR Comment Resolution              |
|  -> If any fixes were made, return to Step 1|
+---------------------------------------------+
```

### Step 1: PR Review

Run the **pr-review-toolkit** plugin to review code quality.

| Item | Value |
|------|-------|
| Plugin | `pr-review-toolkit` |
| Skill | `review-pr` |
| Invocation | `/pr-review-toolkit:review-pr` |

```
/pr-review-toolkit:review-pr
```

- **Fix all** reported issues
- Commit and push after fixes

### Step 2: Security Guidance

The **security-guidance** plugin runs automatically as a `PreToolUse` hook during Edit/Write operations. It does not require manual invocation.

| Item | Value |
|------|-------|
| Plugin | `security-guidance` |
| Type | Automatic (PreToolUse hook) |

- Review any security warnings surfaced during Step 1 edits
- **Fix all** security-related issues
- Commit and push after fixes

### Step 3: Checklist Verification

Verify **all** items listed in the PR description's `Test plan` **locally on the current machine**.

> **重要:** チェックリストの検証は必ず **すべての修正（Step 1〜2 の fix commit）が完了した後** に実施すること。
> 修正前や修正途中の状態で検証しても、修正による副作用やリグレッションを検出できない。

#### 自動検証（必須）

```bash
# 1. verify-pr を実行（必須 — これを実行しないと PR 更新がブロックされる）
task verify-pr PR_NUMBER=<number>
```

- `[PASS]` 項目: 自動検証済み
- `[SKIP]` 項目: 対応するルールがない。手動でコマンドを実行して検証し、結果をユーザーに提示すること
- `[FAIL]` 項目: 修正して再実行

#### ルール

- 各項目を実際にコマンド実行・動作確認して検証する（**目視や推測で通過させない**）
- `verify-pr` の実行結果が `/tmp/verify-pr-result.json` に保存される
- この結果が `pass` でないと `gh pr edit` / `gh api` による PR 更新が **hook でブロック**される
- If all items pass, **immediately** update the PR description (check the boxes)
- If any item fails, fix and re-verify
- **検証と PR description の更新は必ずセットで行うこと。** 検証だけ行って更新を忘れると、PR の状態が実際の検証結果と乖離する

#### 検証ルールの追加

SKIP 項目を自動化したい場合は `scripts/verify-rules.sh` にルールを追加する:

```bash
VERIFY_RULES+=(
  "パターン(grep -iE):::コマンド:::説明"
)
```

### Step 4: PR Comment Resolution

Check all comments on the PR (from reviewers or automated tools).

```bash
# List comments
gh api repos/{owner}/{repo}/pulls/{pr_number}/comments
gh pr view <PR_NUMBER> --comments
```

For each comment:

1. **If a fix is needed**: Implement the fix and reply with the fix details
2. **If no fix is needed**: Reply with the reasoning and resolve the comment

### Loop Execution

If **any fix was made** in Steps 1-4, return to Step 1 and re-run.

Use **ralph-loop** to automate:

| Item | Value |
|------|-------|
| Plugin | `ralph-loop` |
| Skill | `ralph-loop` |
| Invocation | `/ralph-loop:ralph-loop` |

```
/ralph-loop:ralph-loop
```

**Loop exit conditions:**
- Step 1: PR Review reports 0 issues
- Step 2: Security Guidance reports 0 issues
- Step 3: All checklist items pass
- Step 4: 0 unresolved comments

## Notes

- **ALL review issues must be fixed** — Critical, High, and Medium severity issues must all be resolved before merge. No issue may be deferred or skipped regardless of severity level.
- Add review fixes as new commits (do not amend)
- Push after each fix commit
- Address new issues immediately during the loop
- Prioritize security-related issues
