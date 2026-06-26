# fetch / discovery ファイル構成

この文書は fetch / discovery 周辺の file/module 構成規約を定義する。実装はこの規約に従う。

## 設計目標

コード上では、サイトを情報 source の唯一の基準として扱う。discovery、fetch、save、history、translation、PR 作成は、同じパイプライン内の独立した段階として分ける。

`recommend` command は target を解決し、記事処理パイプラインと同じ discovery / fetch の基本部品を呼び出す。

## ディレクトリ構成

```text
src/
  main.rs
  config.rs
  paths.rs
  save.rs
  translate.rs
  target_repos.rs
  youtube.rs

  sites/
    mod.rs
    types.rs
    registry.rs
    hackernews.rs
    devto.rs
    zenn.rs
    twitter.rs
    youtube.rs
    arxiv.rs
    github_advisory.rs
    cisa_kev.rs
    nvd.rs
    aws_whatsnew.rs
    aws_security.rs
    google_cloud_blog.rs
    kubernetes.rs
    cncf.rs
    infoq.rs
    martinfowler.rs
    github_search.rs
    thoughtworks_radar.rs
    doi.rs
    openreview.rs

  fetch/
    mod.rs
    article.rs
    router.rs
    routes/
      mod.rs
      generic_web.rs
      social_status.rs
      video_transcript.rs
      site_article_api.rs

  discovery/
    mod.rs
    types.rs
    planner.rs
    endpoints/
      mod.rs
      rss_feed.rs
      atom_feed.rs
      json_api.rs
      search_api.rs
      catalog_api.rs
      page_links.rs
    xml.rs

  pipeline/
    mod.rs
    history.rs
    artifacts.rs
    translation.rs
    pr.rs
```

## 責務

| 領域 | 責務 | 補足 |
| --- | --- | --- |
| `sites/` | サイト metadata とサイト固有 adapter | サイト名、alias、URL rule、discovery endpoint 設定、candidate parser、article fetch adapter、fetch route、save type を同じ場所に置く。 |
| `fetch/` | route 機構に従って 1 件の記事 URL を取得する | サイト名付きファイルを置かない。サイト固有の記事 API logic は `sites/<site>.rs` が公開し、`site_article_api.rs` 経由で呼び出す。 |
| `discovery/` | endpoint 機構に従ってサイトまたはページを記事候補へ変換する | サイト名付きファイルを置かない。endpoint module は汎用的な feed/API 形状を取得し、`sites/<site>.rs` が所有する parser function を呼び出す。 |
| `pipeline/` | discovery 後の処理 | history filtering、article fetch fan-out、translation、artifact 作成、PR 作成を扱う。 |
| `recommend` command | discovery / pipeline entrypoint | target を解決し、discovery / pipeline 内部処理を呼び出す。 |

## 信頼できる唯一の定義

`sites/registry.rs` だけがサイト一覧全体を組み立てる場所になる。各 `sites/<site>.rs` は、公開 `SITE` constant を 1 つだけ持ち、そのサイト固有の parser や article API adapter も同じファイルで所有する。

サイト固有コードを `fetch/` や `discovery/` に重複配置してはいけない。これらのディレクトリは機構別に整理する。

- `discovery/endpoints/rss_feed.rs`: 汎用 RSS 取得と共通 RSS item mapping。
- `discovery/endpoints/atom_feed.rs`: 汎用 Atom 取得と共通 Atom entry mapping。
- `discovery/endpoints/json_api.rs`: 汎用 JSON API request/response 取得。
- `discovery/endpoints/search_api.rs`: 汎用 query/search API request handling。
- `discovery/endpoints/catalog_api.rs`: 汎用 catalog-style JSON 取得。
- `discovery/endpoints/page_links.rs`: 汎用 page link 抽出。
- `fetch/routes/generic_web.rs`: 汎用 web article fetch。
- `fetch/routes/social_status.rs`: status/tweet 系 article fetch。
- `fetch/routes/video_transcript.rs`: video transcript 系 article fetch。
- `fetch/routes/site_article_api.rs`: site-owned article API adapter への汎用 bridge。

新しいサイトを追加するときは、通常次を行う。

1. `src/sites/<site>.rs` を作成する。
2. `src/sites/mod.rs` に site module を追加する。
3. `src/sites/registry.rs` に `&<site>::SITE` を追加する。
4. 機構別の `DiscoveryEndpoint` variant を追加または再利用する。
5. 機構別の `FetchRoute` variant を追加または再利用する。
6. `docs/sites/<site>.md` を追加する。

## 実装規約

1. site 固有の知識は `sites/<site>.rs` に置く。
2. `discovery/endpoints/` は機構別の candidate 取得処理だけを持つ。
3. `fetch/routes/` は機構別の 1 件記事取得処理だけを持つ。
4. `pipeline/` は discovery 後の history、fetch fan-out、translation、artifact、PR 作成を扱う。
5. CLI command は `sites/registry.rs` と discovery / pipeline の基本部品を使う。
6. 実装は [enums.md](enums.md)、[usage.md](usage.md)、[../sites/README.md](../sites/README.md) の規約にも従う。

## 対象外

- site 固有 file を `fetch/` または `discovery/` 配下に置かない。
- discovery と article fetch を同じ route module に混ぜない。
- `article-collector.toml` の key 名を file/module 構成規約で定義しない。
