use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn source_architecture_uses_module_directories_instead_of_legacy_files() {
    let root = manifest_root();

    assert!(!root.join("src/sites.rs").exists());
    assert!(!root.join("src/fetch.rs").exists());
    assert!(root.join("src/sites/mod.rs").exists());
    assert!(root.join("src/fetch/mod.rs").exists());
    assert!(root.join("src/discovery/mod.rs").exists());
    assert!(root.join("src/pipeline/mod.rs").exists());
}

#[test]
fn mechanism_directories_do_not_contain_site_named_modules() {
    let root = manifest_root();

    assert_eq!(
        file_stems(&root.join("src/fetch/routes")),
        BTreeSet::from([
            "generic_web".to_string(),
            "mod".to_string(),
            "site_article_api".to_string(),
            "social_status".to_string(),
            "video_transcript".to_string(),
        ])
    );
    assert_eq!(
        file_stems(&root.join("src/discovery/endpoints")),
        BTreeSet::from([
            "atom_feed".to_string(),
            "catalog_api".to_string(),
            "json_api".to_string(),
            "mod".to_string(),
            "page_links".to_string(),
            "rss_feed".to_string(),
            "search_api".to_string(),
        ])
    );
}

#[test]
fn site_docs_and_site_modules_have_matching_site_files() {
    let root = manifest_root();
    let docs = file_stems(&root.join("docs/sites"))
        .into_iter()
        .filter(|name| name != "README")
        .map(|name| name.replace('-', "_"))
        .collect::<BTreeSet<_>>();
    let modules = file_stems(&root.join("src/sites"))
        .into_iter()
        .filter(|name| !["mod", "registry", "types"].contains(&name.as_str()))
        .collect::<BTreeSet<_>>();

    assert_eq!(modules, docs);
}

fn manifest_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn file_stems(dir: &Path) -> BTreeSet<String> {
    let mut stems = BTreeSet::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs")
            && path.extension().and_then(|extension| extension.to_str()) != Some("md")
        {
            continue;
        }
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        stems.insert(stem);
    }
    stems
}
