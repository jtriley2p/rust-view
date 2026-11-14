//! Runs the binary on this workspace.

use rust_view_core::Report;
use std::path::Path;
use std::process::Command;

fn reports(path: &str) -> Vec<Report> {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    let output = Command::new(env!("CARGO_BIN_EXE_rust-view"))
        .arg(project)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn names(reports: &[Report]) -> Vec<&str> {
    reports
        .iter()
        .flat_map(|r| &r.crates)
        .map(|c| c.name.as_str())
        .collect()
}

#[test]
fn workspace_reports_each_crate() {
    let reports = reports("../..");
    assert_eq!(names(&reports), ["rust-view-cli", "rust-view-core"]);
    assert!(
        reports
            .iter()
            .all(|r| r.crates.len() == 1 && r.totals == r.crates[0].totals)
    );
    let cli = &reports[0].crates[0];
    assert_eq!(cli.path, "crates/cli");
    assert_eq!(cli.files[0].path, "crates/cli/src/main.rs");
    assert!(
        cli.dependencies
            .iter()
            .any(|d| d.name == "rust-view-core" && d.req.is_none())
    );
}

#[test]
fn crate_reports_itself() {
    assert_eq!(names(&reports(".")), ["rust-view-cli"]);
    assert_eq!(names(&reports("../core/Cargo.toml")), ["rust-view-core"]);
}

#[test]
fn bad_project_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_rust-view"))
        .arg("/nonexistent")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("error: "));
}
