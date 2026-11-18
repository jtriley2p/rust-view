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
fn packages_are_the_resolved_closure() {
    let reports = reports(".");
    let cli = &reports[0].crates[0];
    let has = |name: &str, version: &str| {
        cli.packages
            .iter()
            .any(|p| p.name == name && p.version == version)
    };
    // Itself, a direct dependency, and one reached only through `rust-view-core`.
    assert!(has("rust-view-cli", &cli.version));
    assert!(
        ["rust-view-core", "serde_json", "syn"]
            .iter()
            .all(|name| { cli.packages.iter().any(|p| p.name == *name) })
    );
    assert!(
        cli.packages
            .iter()
            .flat_map(|p| &p.dependencies)
            .all(|d| has(&d.name, &d.version))
    );
    assert!(
        cli.packages
            .windows(2)
            .all(|w| (&w[0].name, &w[0].version) < (&w[1].name, &w[1].version))
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

#[test]
fn input_flag_targets_paths() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rust-view"))
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap()
    };
    let output = run(&["-i", "../core", "--input", "Cargo.toml"]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let reports: Vec<Report> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(names(&reports), ["rust-view-core", "rust-view-cli"]);
    assert_eq!(run(&["-i"]).status.code(), Some(2));
}
