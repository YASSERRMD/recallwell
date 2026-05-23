use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn version_subcommand_prints_version() {
    Command::cargo_bin("recallwell")
        .unwrap()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn version_flag_prints_version() {
    Command::cargo_bin("recallwell")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn help_lists_all_subcommands() {
    Command::cargo_bin("recallwell")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("serve"))
        .stdout(predicate::str::contains("setup"))
        .stdout(predicate::str::contains("config"))
        .stdout(predicate::str::contains("libraries"))
        .stdout(predicate::str::contains("version"));
}

#[test]
fn config_subcommand_prints_path() {
    Command::cargo_bin("recallwell")
        .unwrap()
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("Config path:"));
}

#[test]
fn libraries_subcommand_on_empty_dir() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("recallwell")
        .unwrap()
        .args(["--data-dir"])
        .arg(dir.path())
        .arg("libraries")
        .assert()
        .success()
        .stdout(predicate::str::contains("No libraries"));
}

#[test]
fn libraries_lists_existing_db_files() {
    let dir = tempdir().unwrap();
    let lib_dir = dir.path().join("libraries");
    std::fs::create_dir_all(&lib_dir).unwrap();
    std::fs::write(lib_dir.join("reading.db"), b"x").unwrap();
    std::fs::write(lib_dir.join("work.db"), b"yy").unwrap();
    std::fs::write(lib_dir.join("note.txt"), b"ignored").unwrap();

    Command::cargo_bin("recallwell")
        .unwrap()
        .args(["--data-dir"])
        .arg(dir.path())
        .arg("libraries")
        .assert()
        .success()
        .stdout(predicate::str::contains("reading"))
        .stdout(predicate::str::contains("work"))
        .stdout(predicate::str::contains("note").not());
}
