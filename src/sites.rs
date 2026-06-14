#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchRoute {
    Twitter,
    YouTube,
    HackerNews,
    DevTo,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendSource {
    HackerNewsTopStories { api_url: &'static str },
    DevToArticles { api_url: &'static str },
}

#[derive(Debug, PartialEq, Eq)]
pub struct UrlRule {
    contains: &'static [&'static str],
}

impl UrlRule {
    const fn new(contains: &'static [&'static str]) -> Self {
        Self { contains }
    }

    fn matches(&self, url: &str) -> bool {
        self.contains.iter().all(|part| url.contains(part))
    }
}

#[derive(Debug)]
pub struct Site {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub supported_urls: &'static [&'static str],
    pub fetch_route: FetchRoute,
    fetch_rules: &'static [UrlRule],
    pub save_type: &'static str,
    save_rules: &'static [UrlRule],
    pub recommend: Option<RecommendSource>,
}

const HN_TOPSTORIES_API: &str = "https://hacker-news.firebaseio.com/v0/topstories.json";
const DEVTO_ARTICLES_API: &str = "https://dev.to/api/articles?top=7";

const TWITTER_FETCH_RULES: &[UrlRule] = &[
    UrlRule::new(&["x.com/", "/status/"]),
    UrlRule::new(&["twitter.com/", "/status/"]),
];
const TWITTER_SAVE_RULES: &[UrlRule] =
    &[UrlRule::new(&["x.com/"]), UrlRule::new(&["twitter.com/"])];

const YOUTUBE_FETCH_RULES: &[UrlRule] = &[
    UrlRule::new(&["youtube.com/watch"]),
    UrlRule::new(&["youtu.be/"]),
];
const YOUTUBE_SAVE_RULES: &[UrlRule] = &[
    UrlRule::new(&["youtube.com/"]),
    UrlRule::new(&["youtu.be/"]),
];

const HACKERNEWS_FETCH_RULES: &[UrlRule] = &[UrlRule::new(&["news.ycombinator.com/item"])];
const DEVTO_FETCH_RULES: &[UrlRule] = &[UrlRule::new(&["dev.to/"])];

const ARXIV_SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["arxiv.org/"])];
const DOI_SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["doi.org/"])];
const OPENREVIEW_SAVE_RULES: &[UrlRule] = &[UrlRule::new(&["openreview.net/"])];

pub const SITES: &[Site] = &[
    Site {
        name: "hackernews",
        aliases: &["hn", "hacker-news"],
        supported_urls: &["https://news.ycombinator.com/item?id=<id>"],
        fetch_route: FetchRoute::HackerNews,
        fetch_rules: HACKERNEWS_FETCH_RULES,
        save_type: "web",
        save_rules: &[],
        recommend: Some(RecommendSource::HackerNewsTopStories {
            api_url: HN_TOPSTORIES_API,
        }),
    },
    Site {
        name: "devto",
        aliases: &["dev.to"],
        supported_urls: &["https://dev.to/<author>/<slug>"],
        fetch_route: FetchRoute::DevTo,
        fetch_rules: DEVTO_FETCH_RULES,
        save_type: "web",
        save_rules: &[],
        recommend: Some(RecommendSource::DevToArticles {
            api_url: DEVTO_ARTICLES_API,
        }),
    },
    Site {
        name: "twitter",
        aliases: &["x", "x-twitter"],
        supported_urls: &[
            "https://x.com/<user>/status/<id>",
            "https://twitter.com/<user>/status/<id>",
        ],
        fetch_route: FetchRoute::Twitter,
        fetch_rules: TWITTER_FETCH_RULES,
        save_type: "x",
        save_rules: TWITTER_SAVE_RULES,
        recommend: None,
    },
    Site {
        name: "youtube",
        aliases: &["yt"],
        supported_urls: &[
            "https://www.youtube.com/watch?v=<id>",
            "https://youtu.be/<id>",
        ],
        fetch_route: FetchRoute::YouTube,
        fetch_rules: YOUTUBE_FETCH_RULES,
        save_type: "youtube",
        save_rules: YOUTUBE_SAVE_RULES,
        recommend: None,
    },
    Site {
        name: "arxiv",
        aliases: &["arxiv.org"],
        supported_urls: &["https://arxiv.org/abs/<id>"],
        fetch_route: FetchRoute::Generic,
        fetch_rules: &[],
        save_type: "paper",
        save_rules: ARXIV_SAVE_RULES,
        recommend: None,
    },
    Site {
        name: "doi",
        aliases: &["doi.org"],
        supported_urls: &["https://doi.org/<doi>"],
        fetch_route: FetchRoute::Generic,
        fetch_rules: &[],
        save_type: "paper",
        save_rules: DOI_SAVE_RULES,
        recommend: None,
    },
    Site {
        name: "openreview",
        aliases: &["openreview.net"],
        supported_urls: &["https://openreview.net/forum?id=<id>"],
        fetch_route: FetchRoute::Generic,
        fetch_rules: &[],
        save_type: "paper",
        save_rules: OPENREVIEW_SAVE_RULES,
        recommend: None,
    },
];

pub fn site_by_name(name: &str) -> Option<&'static Site> {
    let normalized = normalize_name(name);
    SITES.iter().find(|site| {
        site.name == normalized || site.aliases.iter().any(|alias| *alias == normalized)
    })
}

pub fn fetch_route_for_url(url: &str) -> FetchRoute {
    SITES
        .iter()
        .find(|site| site.fetch_rules.iter().any(|rule| rule.matches(url)))
        .map(|site| site.fetch_route)
        .unwrap_or(FetchRoute::Generic)
}

pub fn save_type_for_url(url: &str) -> &'static str {
    SITES
        .iter()
        .find(|site| site.save_rules.iter().any(|rule| rule.matches(url)))
        .map(|site| site.save_type)
        .unwrap_or("web")
}

pub fn recommend_source_for_url(url: &str) -> Option<RecommendSource> {
    SITES
        .iter()
        .find(|site| {
            matches!(
                site.recommend,
                Some(RecommendSource::HackerNewsTopStories { .. })
            ) && site.fetch_rules.iter().any(|rule| rule.matches(url))
        })
        .and_then(|site| site.recommend)
}

pub fn recommendable_site_names() -> Vec<&'static str> {
    recommendable_sites()
        .into_iter()
        .map(|site| site.name)
        .collect()
}

pub fn recommendable_sites() -> Vec<&'static Site> {
    SITES
        .iter()
        .filter(|site| site.recommend.is_some())
        .collect()
}

pub fn supported_url_examples() -> Vec<&'static str> {
    SITES
        .iter()
        .flat_map(|site| site.supported_urls.iter().copied())
        .collect()
}

fn normalize_name(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 検証: site 名と alias から同じ Site を解決する
    /// 理由: recommend コマンドでは短い site 名を受け付ける
    /// リスク: `hn` のような短縮名が使えず長い URL 入力が必要になる
    #[test]
    fn resolves_site_by_name_and_alias() {
        assert_eq!(site_by_name("hackernews").unwrap().name, "hackernews");
        assert_eq!(site_by_name("HN").unwrap().name, "hackernews");
        assert_eq!(site_by_name("dev.to").unwrap().name, "devto");
    }

    /// 検証: fetch route は site registry の URL rule から決まる
    /// 理由: サイト別 fetch 分岐を fetch.rs に散らさない
    /// リスク: 新しい site 追加時に複数ファイルを更新し忘れる
    #[test]
    fn resolves_fetch_routes_from_url_rules() {
        assert_eq!(
            fetch_route_for_url("https://news.ycombinator.com/item?id=123"),
            FetchRoute::HackerNews
        );
        assert_eq!(
            fetch_route_for_url("https://dev.to/author/slug"),
            FetchRoute::DevTo
        );
        assert_eq!(
            fetch_route_for_url("https://x.com/user/status/123"),
            FetchRoute::Twitter
        );
        assert_eq!(
            fetch_route_for_url("https://www.youtube.com/watch?v=abc"),
            FetchRoute::YouTube
        );
        assert_eq!(
            fetch_route_for_url("https://example.com/article"),
            FetchRoute::Generic
        );
    }

    /// 検証: 保存分類は site registry の save rule から決まる
    /// 理由: 保存先 `${TYPE}` の判定を save.rs に散らさない
    /// リスク: paper 系 URL が `web` に保存される
    #[test]
    fn resolves_save_type_from_url_rules() {
        assert_eq!(save_type_for_url("https://x.com/user/status/123"), "x");
        assert_eq!(save_type_for_url("https://youtu.be/abc"), "youtube");
        assert_eq!(
            save_type_for_url("https://arxiv.org/abs/2301.12345"),
            "paper"
        );
        assert_eq!(
            save_type_for_url("https://doi.org/10.1234/example"),
            "paper"
        );
        assert_eq!(
            save_type_for_url("https://openreview.net/forum?id=abc"),
            "paper"
        );
        assert_eq!(save_type_for_url("https://example.com/article"), "web");
    }

    /// 検証: HN URL は recommend source に解決される
    /// 理由: HN item URL 入力でも site name 入力でも同じ recommend 経路を使う
    /// リスク: HN URL が generic link 抽出に落ちる
    #[test]
    fn resolves_hackernews_recommend_source_from_url() {
        assert_eq!(
            recommend_source_for_url("https://news.ycombinator.com/item?id=123"),
            Some(RecommendSource::HackerNewsTopStories {
                api_url: HN_TOPSTORIES_API
            })
        );
    }

    /// 検証: PageLinks 型の site 既定 source は URL 入力には適用しない
    /// 理由: `recommend devto` はトップページ起点、`recommend https://dev.to/...` はその URL 起点にしたい
    /// リスク: URL 入力なのに site トップページから収集される
    #[test]
    fn does_not_replace_page_link_urls_with_site_default() {
        assert_eq!(recommend_source_for_url("https://dev.to/author/slug"), None);
    }

    /// 検証: recommend 可能な site 名だけを列挙する
    /// 理由: 未設定 site 名のエラーメッセージで候補を提示する
    /// リスク: 使えない site 名が候補として表示される
    #[test]
    fn lists_recommendable_site_names() {
        assert_eq!(recommendable_site_names(), vec!["hackernews", "devto"]);
    }

    /// 検証: recommend 可能な Site 自体を registry から列挙する
    /// 理由: `recommend all` は個別の名前リストではなく Site 定義から source を取得する
    /// リスク: site 名だけ列挙され、recommend source を安全にたどれない
    #[test]
    fn lists_recommendable_sites() {
        let sites = recommendable_sites();
        assert_eq!(sites.len(), 2);
        assert_eq!(sites[0].name, "hackernews");
        assert_eq!(sites[1].name, "devto");
        assert!(sites.iter().all(|site| site.recommend.is_some()));
    }

    /// 検証: supported URL examples は registry から列挙できる
    /// 理由: URL 対応表を sites.rs に集約し、エラー表示や docs 生成へ再利用できるようにする
    /// リスク: registry に URL 例を持っても実コードから参照されず陳腐化する
    #[test]
    fn lists_supported_url_examples() {
        let examples = supported_url_examples();
        assert!(examples.contains(&"https://news.ycombinator.com/item?id=<id>"));
        assert!(examples.contains(&"https://dev.to/<author>/<slug>"));
    }
}
