# AGENTS.md

このリポジトリで作業するすべてのエージェントは、このファイルのルールに従うこと。

## 実装計画と GitHub Issues

実装計画を立てた場合は、コード編集・実装・PR 作成を始める前に、このリポジトリの GitHub Issues に計画を記録すること。

### 必須手順

1. 既存 issue がある場合は、その issue に実装計画を反映する
2. 既存 issue がない場合は、新規 issue を作成する
3. 作業単位が複数ある場合は、GitHub の sub-issue または issue 本文のチェックリストに分解する
4. issue には目的、実装範囲、受け入れ条件、検証コマンド、必要な release/cleanup 作業を記載する
5. GitHub issue への記載が完了してから実装に着手する
6. PR の本文に issue へのリンクを記載する。マージで issue が完了する場合は `Closes #<number>` を使う
7. 作業結果を報告する前に、issue と sub-issue のチェックリストや状態を実態に合わせて更新する

### 例外

ユーザーが明示的に issue への記載が不要と指示した場合のみ省略できる。省略した理由を最終報告に明記すること。

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
- `/tmp/verify-pr-result.json` の `status` が `pass` であることを確認してから PR 本文のチェックボックスを更新する

### 検証ルールの追加

自動検証したい項目が SKIP になった場合は `scripts/verify-rules.sh` にルールを追加する。

```bash
# フォーマット: "パターン(grep -iE):::コマンド:::説明"
VERIFY_RULES+=(
  "新しいパターン:::検証コマンド:::説明"
)
```

## CI の確認

PR の作成または更新後に `gh pr checks <PR_NUMBER> --watch` で GitHub Actions の完了まで待機する。

- CI が成功したら [PR レビュープロセス](docs/pr/README.md) に進む。
- CI が失敗したら `gh run view <RUN_ID> --log-failed` で原因を確認し、修正を push して再確認する。

## コーディング規約

- Rust stable toolchain を使用
- `cargo fmt`, `cargo clippy` を通すこと
- テスト: `cargo test --locked`

## ブランチ命名

- `feat/xxx` — 新機能
- `fix/xxx` — バグ修正
- `docs/xxx` — ドキュメント変更
- `refactor/xxx` — リファクタリング
