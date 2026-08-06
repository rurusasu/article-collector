# 設計・実装計画ドキュメント

`plans/` と `specs/` の日付付き Markdown は、その時点の設計判断・実装手順を残す
履歴スナップショットである。本文中の未実装前提、旧 API、旧 issue 番号、チェック
ボックスは現在の実装状態を表さない。

現行仕様は次の文書を正本とする。

- 利用者向け導線: [../../README.md](../../README.md)
- module 関係: [../architectures/files.md](../architectures/files.md)、[../architectures/usage.md](../architectures/usage.md)
- plugin runtime/lifecycle: [../plugins/README.md](../plugins/README.md)、配布: [../plugins/distribution.md](../plugins/distribution.md)
- X plugin: [../sites/twitter.md](../sites/twitter.md)
- CI/CD: [../ci-cd/README.md](../ci-cd/README.md)
- テスト: [../tests/README.md](../tests/README.md)

## 現行状態

- core release: `v0.6.23`
- X plugin manifest: core release tag と同期
- X plugin: signed production registry から install 可能
- X direct status fetch: core built-in のまま、plugin install 不要
- registry fixture: `registry/plugins-v1.json` は PR 検証用の空 fixture

履歴文書を更新する場合は本文を現在の仕様に書き換えず、このファイルまたは文書先頭
に status を追記して、履歴としての意味を保持する。
