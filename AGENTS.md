# AGENTS.md

## 実装計画と Plane issue 管理

実装計画を立てた場合は、コード編集・実装・PR 作成を始める前に、必ず Plane に対応する issue を作成または更新すること。

### 必須手順

1. 既存 issue がある場合は、その issue に実装計画を反映する
2. 既存 issue がない場合は、新規 issue を作成する
3. 実装計画の作業単位が複数ある場合は、Plane の sub-issue または checklist 相当の粒度で分解する
4. issue には目的、実装範囲、受け入れ条件、検証コマンド、必要な release/cleanup 作業を記載する
5. Plane への記載が完了してから実装に着手する
6. 作業完了後は、該当 issue/sub-issue の status を実態に合わせて更新する

### 禁止事項

- 実装計画を立てたのに Plane へ記載しないまま実装を開始することは**禁止**
- Plane issue の status を未更新のまま「完了」と報告することは**禁止**
- release、branch cleanup、verification が必要な作業を issue から漏らすことは**禁止**

### 例外

ユーザーが明示的に「Plane への記載不要」「issue を作らない」などと指示した場合のみ例外とする。その場合でも、最終報告で Plane 更新を省略した理由を明記すること。

## PR チェックリスト検証ルール

PR レビューの Step 3（チェックリスト検証）では、以下のプロセスを**必ず**実行すること。

### 必須手順

1. `task verify-pr PR_NUMBER=<number>` を実行する
2. 全項目が PASS であることを確認する
3. SKIP 項目がある場合は、該当項目を手動で検証し、コマンド実行結果をユーザーに提示する
4. 上記が完了してから PR description のチェックボックスを更新する

### 禁止事項

- 「目視確認した」「コード上問題ない」等の自己判断による検証スキップは**禁止**
- verify-pr を実行せずにチェックボックスを更新することは**禁止**
- `/tmp/verify-pr-result.json` が `pass` でない状態での PR body 更新は hook によりブロックされる

### 検証ルールの追加

自動検証したい項目が SKIP になった場合は `scripts/verify-rules.sh` にルールを追加する。

```bash
# フォーマット: "パターン(grep -iE):::コマンド:::説明"
VERIFY_RULES+=(
  "新しいパターン:::検証コマンド:::説明"
)
```

## CI 自動監視

`git push` 後に PostToolUse hook が `scripts/wait-ci.sh` を自動実行し、CI 完了まで待機する（最大5分）。

- CI pass → PR レビュープロセスに進む
- CI fail → 失敗ログが自動的にコンテキストに注入される。**CI failure は即座に修正すること**
- タイムアウト → `gh pr checks <PR_NUMBER> --watch` で手動監視

この仕組みは hook により自動実行されるため、手動での CI 確認は不要。

## コーディング規約

- Rust stable toolchain を使用
- `cargo fmt`, `cargo clippy` を通すこと
- テスト: `cargo test --locked`

## ブランチ命名

- `feat/xxx` — 新機能
- `fix/xxx` — バグ修正
- `docs/xxx` — ドキュメント変更
- `refactor/xxx` — リファクタリング
