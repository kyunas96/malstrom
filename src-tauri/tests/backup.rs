use malstrom_lib::als::output_path::backup_before_overwrite;
use std::fs;

#[test]
fn refuses_when_backup_dir_missing() {
    let dir = std::env::temp_dir().join(format!("malstrom-backup-test-missing-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let src = dir.join("Project.als");
    fs::write(&src, b"fake gzip bytes").unwrap();

    let err = backup_before_overwrite(&src).unwrap_err();
    assert!(err.to_string().contains("No Backup/"));

    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn copies_bytes_with_expected_name_pattern() {
    let dir = std::env::temp_dir().join(format!("malstrom-backup-test-ok-{}", std::process::id()));
    let backup_dir = dir.join("Backup");
    fs::create_dir_all(&backup_dir).unwrap();
    let src = dir.join("Project.als");
    fs::write(&src, b"fake gzip bytes").unwrap();

    let backup_path = backup_before_overwrite(&src).unwrap();
    assert_eq!(fs::read(&backup_path).unwrap(), b"fake gzip bytes");
    let name = backup_path.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with("Project ["), "unexpected name: {name}");
    assert!(name.ends_with("].als"), "unexpected name: {name}");

    fs::remove_dir_all(&dir).unwrap();
}
