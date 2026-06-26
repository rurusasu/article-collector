# file 間の利用関係

この文書は、fetch / discovery 周辺の file どうしがどうつながるかを説明する。配置の詳細は [files.md](files.md)、型の構成規約は [enums.md](enums.md)、各 site の URL 構造と取得方法は [../sites/README.md](../sites/README.md) を参照する。

## docs の役割分担

| file | 所有する情報 | 他 docs との関係 |
| --- | --- | --- |
| [files.md](files.md) | 目標 directory / module 構成。 | file の置き場所を変えるときに更新する。 |
| [enums.md](enums.md) | `Site`、`DiscoveryEndpoint`、`FetchRoute`、`SaveType`、`ArticleCandidate` の型構造。 | field や enum variant を変えるときに更新する。 |
| [usage.md](usage.md) | module 間の呼び出し順、所有側と利用側の関係。 | pipeline のつながりや責務境界を変えるときに更新する。 |
| [../sites/<site>.md](../sites/README.md) | site ごとの URL 構造、discovery endpoint、article fetch、保存分類。 | site 固有の知識を変えるときに更新する。 |
| [../AGENTS.md](../AGENTS.md) | docs 全体の索引。 | docs を追加・削除したときに更新する。 |

## 実行時の流れ

```text
CLI / config
  -> sites/registry.rs
  -> discovery/planner.rs
  -> discovery/endpoints/<mechanism>.rs
  -> sites/<site>.rs の parse_discovery
  -> ArticleCandidate
  -> pipeline/history.rs
  -> fetch/router.rs
  -> fetch/routes/<mechanism>.rs
  -> sites/<site>.rs の fetch_article
  -> save.rs
  -> pipeline/translation.rs
  -> pipeline/artifacts.rs
  -> pipeline/pr.rs
```

`recommend` command は target 解決後に `discovery/planner.rs` と pipeline を呼び出し、独自の source namespace を持たない。

## 所有側と利用側

| 所有 file | 所有する責務 | 利用する file |
| --- | --- | --- |
| `src/sites/<site>.rs` | site 名、alias、URL rule、discovery endpoint 設定、candidate parser、site 固有 article API adapter。 | `src/sites/registry.rs`、`src/discovery/planner.rs`、`src/discovery/endpoints/*.rs`、`src/fetch/routes/site_article_api.rs`。 |
| `src/sites/registry.rs` | 全 site の一覧と site lookup。 | CLI target 解決、`src/discovery/planner.rs`、URL classification、save type 判定。 |
| `src/sites/types.rs` | `Site`、`UrlRule`、`DiscoveryEndpoint`、`FetchRoute`、`SaveType` などの共有型。 | `sites/`、`discovery/`、`fetch/`、`save.rs`。 |
| `src/discovery/planner.rs` | target と config から discovery 実行計画を作る。 | `recommend` command、将来の article pipeline command。 |
| `src/discovery/endpoints/*.rs` | RSS、Atom、JSON API、search API、catalog API、page links など機構別の取得処理。 | `src/discovery/planner.rs` から呼ばれ、必要に応じて `sites/<site>.rs` の parser に payload を渡す。 |
| `src/fetch/router.rs` | URL または candidate から fetch route を選ぶ。 | pipeline の article fetch fan-out。 |
| `src/fetch/routes/*.rs` | generic web、social status、video transcript、site article API など機構別の単一記事 fetch。 | `src/fetch/router.rs` から呼ばれる。 |
| `src/save.rs` | fetch 済み content の Markdown 保存分類。 | pipeline の保存処理。 |
| `src/pipeline/*.rs` | history、artifact、translation、PR 作成など discovery/fetch 後の処理。 | CLI command。 |

## site 固有処理の流し方

site 固有の処理は、必ず `src/sites/<site>.rs` を起点にする。

1. discovery endpoint の種類は `Site.discovery` に設定する。
2. endpoint module は feed/API/page を機構別に取得する。
3. response の site 固有 parse は `Site.parse_discovery` に委譲する。
4. 生成された `ArticleCandidate.site` は source identity として扱う。
5. 1 件の記事取得で site 固有 API が必要な場合は `Site.fetch_article` に委譲する。

`src/discovery/endpoints/` と `src/fetch/routes/` には、`hackernews.rs` や `devto.rs` のような site 名付き file を置かない。site 名で分ける必要が出た場合は、まず `src/sites/<site>.rs` に寄せられないか確認する。

## 変更時に見る場所

| 変更内容 | 先に見る file | 一緒に更新する docs |
| --- | --- | --- |
| site を追加する | `src/sites/<site>.rs`、`src/sites/registry.rs` | [files.md](files.md)、[../sites/README.md](../sites/README.md)、`docs/sites/<site>.md`、[../AGENTS.md](../AGENTS.md)。 |
| URL match を変える | `src/sites/<site>.rs` | 対象の `docs/sites/<site>.md`。 |
| discovery endpoint を増やす | `src/sites/types.rs`、`src/discovery/endpoints/` | [enums.md](enums.md)、[files.md](files.md)、対象の site doc。 |
| fetch route を増やす | `src/sites/types.rs`、`src/fetch/routes/` | [enums.md](enums.md)、[files.md](files.md)、対象の site doc。 |
| `recommend` の config 規約を変える | `src/config.rs`、CLI command | [enums.md](enums.md)、README、必要なら site docs。 |
| pipeline の順序を変える | `src/pipeline/` | この [usage.md](usage.md)。 |

## 境界の確認

- site 固有の URL 構造、API response の parse、article API adapter は `sites/<site>.rs` に置く。
- `discovery/endpoints/*.rs` は取得機構の共有処理だけを持つ。
- `fetch/routes/*.rs` は 1 件の記事取得の共有処理だけを持つ。
- `ArticleCandidate.site` を source identity とし、別の `source` field は増やさない。
- `recommend` と将来の pipeline command は同じ discovery / fetch primitive を使う。
