use malstrom_lib::list_projects_in as list_projects;

#[path = "support/mod.rs"]
mod support;

/// Builds a fresh root dir containing two `.als` fixtures, a nested `.als`
/// (which listing must ignore -- only top-level files count) and a non-`.als`
/// file, mirroring the on-disk layout `list_projects_in` is meant to scan.
fn build_list_projects_root(label: &str) -> std::path::PathBuf {
    let dir = support::temp_dir(label);
    support::write_als_fixture(&dir.join("10-30-25.als"), support::LIVE_12_STYLE_XML);
    support::write_als_fixture(&dir.join("5-27-20.als"), support::LIVE_9_STYLE_XML);
    support::write_als_fixture(&dir.join("nested").join("9-25-18.als"), support::EMPTY_XML);
    std::fs::write(dir.join("notes.txt"), "not an als file").unwrap();
    dir
}

#[test]
fn lists_als_files_directly_in_root_ignoring_nested_and_non_als() {
    let dir = build_list_projects_root("lists-als-files");
    let projects = list_projects(dir.to_string_lossy().to_string()).unwrap();

    let names: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["10-30-25", "5-27-20"]);
}

#[test]
fn each_project_has_path_and_scale_candidates() {
    let dir = build_list_projects_root("each-project-has-path");
    let projects = list_projects(dir.to_string_lossy().to_string()).unwrap();

    for project in &projects {
        assert!(project.path.ends_with(".als"));
        assert!(!project.scales.is_empty());
    }
}

#[test]
fn missing_root_folder_returns_err() {
    let result = list_projects("tests/fixtures/does-not-exist".to_string());
    assert!(result.is_err());
}

#[test]
fn empty_folder_returns_empty_list() {
    let dir = support::temp_dir("empty-folder-returns-empty-list");

    let projects = list_projects(dir.to_string_lossy().to_string()).unwrap();
    assert!(projects.is_empty());
}
