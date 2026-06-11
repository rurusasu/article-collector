use std::path::PathBuf;

pub const OUTDIR_ENV: &str = "ARTICLE_COLLECTOR_OUTDIR";

pub fn outdir() -> PathBuf {
    if let Some(path) = env_path(OUTDIR_ENV) {
        return path;
    }

    if cfg!(windows) {
        std::env::temp_dir().join("article-collector")
    } else {
        PathBuf::from("/tmp/collect")
    }
}

pub fn raw_json_path() -> PathBuf {
    outdir().join("raw.json")
}

pub fn translated_md_path() -> PathBuf {
    outdir().join("translated.md")
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
    fn output_files_are_under_outdir() {
        let outdir = outdir();
        assert_eq!(raw_json_path(), outdir.join("raw.json"));
        assert_eq!(translated_md_path(), outdir.join("translated.md"));
    }
}
