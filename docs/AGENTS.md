# Agent 向けドキュメント索引

このファイルは、source 管理で参照する architecture docs と site docs の索引である。詳細な挙動はリンク先に置き、このファイルは compact な map として保つ。

## アーキテクチャドキュメント一覧

| ファイル | 要約 | 使う場面 |
| --- | --- | --- |
| [architectures/files.md](architectures/files.md) | サイト固有 code を `sites/` に、機構別 code を `discovery/` / `fetch/` に置く file/module 構成規約。 | fetch/discovery の file/module 構成を確認するとき。 |
| [architectures/enums.md](architectures/enums.md) | `Site`、機構別 `DiscoveryEndpoint`、機構別 `FetchRoute`、`SaveType`、`ArticleCandidate` の理想的な enum/type model。 | discovery endpoint、fetch route、typed save category を変更するとき。 |
| [architectures/usage.md](architectures/usage.md) | docs と実装 module のつながり、所有側と利用側、実行時の流れ。 | file 間の関係、pipeline の順序、site 固有処理の流し方を確認するとき。 |

## サイトドキュメント一覧

| ファイル | site | discovery | fetch | 要約 |
| --- | --- | --- | --- | --- |
| [sites/hackernews.md](sites/hackernews.md) | `hackernews` | `DiscoveryEndpoint::JsonApi` | `FetchRoute::SiteArticleApi` | top story ID を discovery し、item JSON から外部記事 URL を得る。 |
| [sites/devto.md](sites/devto.md) | `devto` | `DiscoveryEndpoint::JsonApi` | `FetchRoute::SiteArticleApi` | public API から記事 metadata と直接記事 URL を得る。 |
| [sites/zenn.md](sites/zenn.md) | `zenn` | `DiscoveryEndpoint::RssFeed` | `FetchRoute::GenericWeb` | Zenn feed から記事候補を得て、ページは generic web fetch で取得する。 |
| [sites/twitter.md](sites/twitter.md) | `twitter` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::SocialStatus` | X API v2 recent search で post 候補を得て、status URL は social status fetch で扱う。 |
| [sites/qiita.md](sites/qiita.md) | `qiita` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::GenericWeb` | Qiita API v2 items search から技術記事候補を得る。 |
| [sites/bluesky.md](sites/bluesky.md) | `bluesky` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::GenericWeb` | public AppView searchPosts から post URL を得る。 |
| [sites/youtube.md](sites/youtube.md) | `youtube` | なし | `FetchRoute::VideoTranscript` | video URL の直接 fetch のみ対応する。将来の channel discovery はこの site に追加する。 |
| [sites/arxiv.md](sites/arxiv.md) | `arxiv` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::GenericWeb` | `SearchRequest::ArxivSearch` を使う paper discovery endpoint。 |
| [sites/github-advisory.md](sites/github-advisory.md) | `github-advisory` | `DiscoveryEndpoint::JsonApi` | `FetchRoute::GenericWeb` | GHSA/CVE metadata を持つ security advisory discovery。 |
| [sites/cisa-kev.md](sites/cisa-kev.md) | `cisa-kev` | `DiscoveryEndpoint::CatalogApi` | `FetchRoute::GenericWeb` | catalog discovery から NVD CVE detail URL を生成する。 |
| [sites/nvd.md](sites/nvd.md) | `nvd` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::GenericWeb` | queryable CVE discovery endpoint。 |
| [sites/aws-whatsnew.md](sites/aws-whatsnew.md) | `aws-whatsnew` | `DiscoveryEndpoint::RssFeed` | `FetchRoute::GenericWeb` | AWS product update feed と article page。 |
| [sites/aws-security.md](sites/aws-security.md) | `aws-security` | `DiscoveryEndpoint::RssFeed` | `FetchRoute::GenericWeb` | AWS security bulletin feed と page。 |
| [sites/google-cloud-blog.md](sites/google-cloud-blog.md) | `google-cloud-blog` | `DiscoveryEndpoint::RssFeed` | `FetchRoute::GenericWeb` | feed host と article host が異なるため、site が両方を所有する。 |
| [sites/kubernetes.md](sites/kubernetes.md) | `kubernetes` | `DiscoveryEndpoint::RssFeed` | `FetchRoute::GenericWeb` | feed discovery から Kubernetes の日付付き blog URL を得る。 |
| [sites/cncf.md](sites/cncf.md) | `cncf` | `DiscoveryEndpoint::RssFeed` | `FetchRoute::GenericWeb` | CNCF feed discovery と generic web fetch。 |
| [sites/infoq.md](sites/infoq.md) | `infoq` | `DiscoveryEndpoint::RssFeed` | `FetchRoute::GenericWeb` | RSS discovery から幅広い InfoQ URL family を扱う。 |
| [sites/martinfowler.md](sites/martinfowler.md) | `martinfowler` | `DiscoveryEndpoint::AtomFeed` | `FetchRoute::GenericWeb` | Atom feed discovery と generic web fetch。 |
| [sites/github-search.md](sites/github-search.md) | `github-search` | `DiscoveryEndpoint::SearchApi` | `FetchRoute::GenericWeb` | queryable repository discovery endpoint。 |
| [sites/thoughtworks-radar.md](sites/thoughtworks-radar.md) | `thoughtworks-radar` | なし | `FetchRoute::GenericWeb` | 安定した feed/API ができるまでは manual/direct fetch のみ扱う。 |
| [sites/doi.md](sites/doi.md) | `doi` | なし | `FetchRoute::GenericWeb` | DOI URL の直接 fetch と `SaveType::Paper`。 |
| [sites/openreview.md](sites/openreview.md) | `openreview` | なし | `FetchRoute::GenericWeb` | OpenReview page の直接 fetch と `SaveType::Paper`。 |

## 保守ルール

| 変更 | 更新する docs |
| --- | --- |
| site を追加する | `docs/sites/<site>.md` を追加し、[sites/README.md](sites/README.md) とこの index を更新する。 |
| discovery 機構を追加する | [architectures/enums.md](architectures/enums.md) と対象 site doc を更新する。site-specific parsing は `sites/<site>.rs` に置く。 |
| 目標 file layout を変更する | [architectures/files.md](architectures/files.md) を更新する。 |
| file 間のつながりを変更する | [architectures/usage.md](architectures/usage.md) を更新する。 |
| URL matching または fetch route を変更する | 対象 site doc を更新する。 |
| CLI/config の規約を変更する | architecture docs と README を一緒に更新する。 |
