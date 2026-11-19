//! Runs the binary on this workspace.

use rust_view_core::Report;
use std::path::Path;
use std::process::Command;

fn reports(path: &str) -> Vec<Report> {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    let output = Command::new(env!("CARGO_BIN_EXE_rust-view"))
        .arg(project)
        .args(["-o", "-"])
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
    assert!(cli.files.iter().any(|f| f.path == "crates/cli/src/main.rs"));
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
    let output = run(&["-i", "../core", "--input", "Cargo.toml", "-o", "-"]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    let reports: Vec<Report> = stdout
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(names(&reports), ["rust-view-core", "rust-view-cli"]);
    assert_eq!(run(&["-i"]).status.code(), Some(2));
}

#[test]
fn no_arguments_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_rust-view"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).starts_with("usage: "));
}

/// The value a store script hands the page: `rustView.<call>(<value>)`.
fn handed(path: &Path, call: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(path).unwrap();
    let json = text
        .trim()
        .strip_prefix(&format!("rustView.{call}("))
        .and_then(|rest| rest.strip_suffix(")"))
        .unwrap();
    serde_json::from_str(json).unwrap()
}

#[test]
fn store_keeps_one_entry_per_project() {
    let home = Path::new(env!("CARGO_TARGET_TMPDIR")).join("store");
    let _ = std::fs::remove_dir_all(&home);
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_rust-view"))
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("RUST_VIEW_HOME", &home)
            .env("BROWSER", "true")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
    };
    // Opening alone installs the page and reports on nothing.
    run(&["--open"]);
    assert!(home.join("index.html").exists());
    assert!(!home.join("index.js").exists());

    // A project run again replaces its entry and moves to the end.
    run(&["../..", "."]);
    run(&["../..", "--open"]);
    let index = handed(&home.join("index.js"), "index");
    let entries = index.as_array().unwrap();
    let listed: Vec<_> = entries
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert_eq!(listed, ["cli", "rust-view"]);
    assert_eq!(
        entries[1]["crates"],
        serde_json::json!(["rust-view-cli", "rust-view-core"])
    );

    let id = entries[1]["id"].as_str().unwrap();
    let script = home.join("reports").join(format!("{id}.js"));
    let text = std::fs::read_to_string(&script).unwrap();
    let (key, reports) = text
        .trim()
        .strip_prefix("rustView.report(")
        .and_then(|rest| rest.strip_suffix(")"))
        .and_then(|rest| rest.split_once(','))
        .unwrap();
    assert_eq!(key, format!("\"{id}\""));
    let reports: Vec<Report> = serde_json::from_str(reports).unwrap();
    assert_eq!(names(&reports), ["rust-view-cli", "rust-view-core"]);

    // An entry whose script is gone leaves the index on the next run.
    std::fs::remove_file(script).unwrap();
    run(&["."]);
    assert_eq!(
        handed(&home.join("index.js"), "index")
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

/// The page reads the store as it loads, so the report has to be there first.
#[cfg(unix)]
#[test]
fn open_follows_the_report() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let home = tmp.join("open-store");
    let _ = std::fs::remove_dir_all(&home);
    // A browser that fails unless the index is beside the page it is given.
    let browser = tmp.join("browser.sh");
    std::fs::write(
        &browser,
        "#!/bin/sh\ntest -f \"$(dirname \"$1\")/index.js\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rust-view"))
        .args([".", "--open"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUST_VIEW_HOME", &home)
        .env("BROWSER", &browser)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // No terminal, no spinner.
    assert!(output.stderr.is_empty());
}
