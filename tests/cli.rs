use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn bare_command_shows_help_without_loading_settings() {
    Command::cargo_bin("hnbot")
        .unwrap()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"))
        .stderr(predicate::str::contains("serve"));
}

#[test]
fn main_command_is_rejected_without_loading_settings() {
    Command::cargo_bin("hnbot")
        .unwrap()
        .arg("main")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand 'main'"));
}

#[test]
fn serve_help_does_not_load_settings() {
    Command::cargo_bin("hnbot")
        .unwrap()
        .args(["serve", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--poll-interval"));
}
