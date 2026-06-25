use std::path::PathBuf;

pub const TEMP_DIR_ENV: &str = "ARTICLE_COLLECTOR_TEMP_DIR";
pub const OUTPUT_DIR_ENV: &str = "ARTICLE_COLLECTOR_OUTPUT_DIR";

pub fn temp_dir() -> PathBuf {
    if let Some(path) = env_path(TEMP_DIR_ENV) {
        return path;
    }

    if cfg!(windows) {
        std::env::temp_dir().join("article-collector")
    } else {
        PathBuf::from("/tmp/collect")
    }
}

pub fn raw_json_path() -> PathBuf {
    temp_dir().join("raw.json")
}

pub fn translated_md_path() -> PathBuf {
    temp_dir().join("translated.md")
}

pub fn recommended_articles_dir() -> PathBuf {
    temp_dir().join("recommended_articles")
}

pub fn output_dir() -> Option<PathBuf> {
    env_path(OUTPUT_DIR_ENV)
}

pub fn recommend_fetch_failures_path() -> PathBuf {
    temp_dir().join("recommend-fetch-failures.json")
}

pub fn default_target_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::temp_dir().join("article-collector-target-repo")
    } else {
        PathBuf::from("/tmp/target-repo")
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).and_then(|value| {
        if value.is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 検証: 出力ファイルが作業ディレクトリ配下に解決される
    /// 理由: raw.json と translated.md は同一の作業ディレクトリを共有する必要がある
    /// リスク: コマンド間で異なる場所を参照し、collect パイプラインが途中で失敗する
    #[test]
    fn run_artifact_files_are_under_temp_dir() {
        let temp_dir = temp_dir();
        assert_eq!(raw_json_path(), temp_dir.join("raw.json"));
        assert_eq!(translated_md_path(), temp_dir.join("translated.md"));
    }

    #[test]
    fn recommend_article_artifact_paths_are_under_temp_dir() {
        let temp_dir = temp_dir();
        assert_eq!(
            recommended_articles_dir(),
            temp_dir.join("recommended_articles")
        );
        assert_eq!(
            recommend_fetch_failures_path(),
            temp_dir.join("recommend-fetch-failures.json")
        );
    }

    #[test]
    fn temp_dir_env_name_describes_temporary_artifacts() {
        assert_eq!(TEMP_DIR_ENV, "ARTICLE_COLLECTOR_TEMP_DIR");
    }

    #[test]
    fn output_dir_env_name_describes_final_outputs() {
        assert_eq!(OUTPUT_DIR_ENV, "ARTICLE_COLLECTOR_OUTPUT_DIR");
    }
}
