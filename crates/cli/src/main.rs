//! `rust-view`: one JSON report per crate of the given cargo projects, kept in a store the
//! dashboard reads.

use rust_view_core::report::{self, DepKind, DepSource, Dependency};
use rust_view_core::{CrateInput, Report, Source, analyze};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{env, fmt, fs, io, thread};

const USAGE: &str = "\
usage: rust-view [-i PATH]... [-o DIR] [--open] [PATH]...

Reports on cargo projects, one report per crate, and keeps them in ~/.rust-view for the dashboard.
PATH is a cargo project directory or its Cargo.toml.

  -i, --input PATH  report on PATH; repeatable, same as a bare PATH
  -o, --out DIR     write DIR/<crate>.json instead; `-` prints one report per line
      --open        open the dashboard in the browser, after any reports are written
  -h, --help        print this help

$RUST_VIEW_HOME replaces ~/.rust-view, and $BROWSER the system's opener.
";

/// The dashboard as one self-contained page, or a stub if it was not built (see `build.rs`).
const PAGE: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

/// What wraps the store's index in `index.js`. A `file://` page can run a script beside it but
/// cannot fetch or list files, so the store is scripts that hand their JSON to the page.
const INDEX: [&str; 2] = ["rustView.index(", ")"];

#[derive(Debug)]
enum Error {
    Usage(String),
    /// `cargo metadata` could not run or failed.
    Cargo {
        manifest: PathBuf,
        message: String,
    },
    /// `cargo metadata` printed something unexpected.
    Metadata(serde_json::Error),
    /// The manifest is not a package of its own workspace.
    NoPackage(PathBuf),
    /// The file is outside the workspace, so it has no workspace-relative path.
    Outside(PathBuf),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Encode(serde_json::Error),
    /// There is no home directory to keep the store in.
    NoHome,
    /// The dashboard was written to `page` but the browser did not open it.
    Open {
        page: PathBuf,
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Usage(message) => write!(f, "{message}"),
            Error::Cargo { manifest, message } => {
                write!(f, "{}: cargo metadata: {message}", manifest.display())
            }
            Error::Metadata(e) => write!(f, "cargo metadata: unexpected output: {e}"),
            Error::NoPackage(manifest) => write!(f, "{}: no package found", manifest.display()),
            Error::Outside(path) => write!(f, "{}: outside the workspace", path.display()),
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Error::Encode(e) => write!(f, "encoding report: {e}"),
            Error::NoHome => write!(f, "no home directory; set RUST_VIEW_HOME"),
            Error::Open { page, message } => {
                write!(f, "{}: could not open: {message}", page.display())
            }
        }
    }
}

/// The parts of `cargo metadata` used here.
#[derive(Deserialize)]
struct Metadata {
    /// Workspace members and, when resolved, every package they depend on.
    packages: Vec<Package>,
    workspace_members: Vec<String>,
    /// `None` under `--no-deps`.
    resolve: Option<Resolve>,
    workspace_root: PathBuf,
    target_directory: PathBuf,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    source: Option<String>,
    manifest_path: PathBuf,
    dependencies: Vec<Dep>,
    targets: Vec<Target>,
}

#[derive(Deserialize)]
struct Dep {
    name: String,
    req: String,
    kind: Option<DepKind>,
    source: Option<String>,
}

#[derive(Deserialize)]
struct Target {
    src_path: PathBuf,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<NodeDep>,
}

#[derive(Deserialize)]
struct NodeDep {
    pkg: String,
    dep_kinds: Vec<NodeDepKind>,
}

#[derive(Deserialize)]
struct NodeDepKind {
    kind: Option<DepKind>,
}

/// What is under way, which a terminal's stderr shows beside a spinner. Empty when nothing is.
static STATUS: Mutex<String> = Mutex::new(String::new());

/// The status with the spinner's line wiped, so what is printed while it is held starts a line.
fn wiped() -> MutexGuard<'static, String> {
    let status = STATUS.lock().unwrap_or_else(PoisonError::into_inner);
    if !status.is_empty() && io::stderr().is_terminal() {
        let _ = write!(io::stderr(), "\r{:1$}\r", "", status.chars().count() + 2);
    }
    status
}

/// Says what is under way, or with `""` that nothing is.
fn status(text: &str) {
    *wiped() = text.into();
}

/// Prints a line to stderr, clear of the spinner.
fn note(line: &str) {
    let _held = wiped();
    let _ = writeln!(io::stderr(), "{line}");
}

/// Draws the spinner for as long as there is a status, if stderr is a terminal.
fn spin() {
    if !io::stderr().is_terminal() {
        return;
    }
    thread::spawn(|| {
        for frame in "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏".chars().cycle() {
            {
                let status = STATUS.lock().unwrap_or_else(PoisonError::into_inner);
                if !status.is_empty() {
                    let _ = write!(io::stderr(), "\r{frame} {status}");
                }
            }
            thread::sleep(Duration::from_millis(80));
        }
    });
}

/// One project in the store's index, which runs oldest first.
#[derive(Serialize, Deserialize)]
struct Entry {
    /// Names the project's script, `reports/<id>.js`.
    id: String,
    name: String,
    /// The project directory.
    path: String,
    /// When the reports were written, in milliseconds since the epoch.
    time: u64,
    /// The reports' schema version.
    version: u32,
    crates: Vec<String>,
}

fn main() -> ExitCode {
    let (mut paths, mut out, mut open) = (Vec::new(), None, false);
    let mut args = env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let usage = match arg.to_str() {
            Some("-h" | "--help") => {
                print!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            Some("-i" | "--input") => match args.next() {
                Some(path) => {
                    paths.push(PathBuf::from(path));
                    continue;
                }
                None => Error::Usage("-i needs a path".into()),
            },
            Some("-o" | "--out") => match args.next() {
                Some(dir) => {
                    out = Some(PathBuf::from(dir));
                    continue;
                }
                None => Error::Usage("-o needs a directory".into()),
            },
            Some("--open") => {
                open = true;
                continue;
            }
            Some("-") => Error::Usage("`-` is not a path; use `-o -` for stdout".into()),
            Some(flag) if flag.starts_with('-') => Error::Usage(format!("unknown option `{flag}`")),
            _ => {
                paths.push(PathBuf::from(arg));
                continue;
            }
        };
        eprintln!("error: {usage}\n\n{USAGE}");
        return ExitCode::from(2);
    }
    if paths.is_empty() && !open {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    }
    spin();
    let exports = paths.iter().map(|path| export(path, out.as_deref()));
    // Lazy, so the page opens once every report it is to show is in the store.
    let opened = open.then_some(()).into_iter().map(|()| {
        status("");
        launch()
    });
    let mut code = ExitCode::SUCCESS;
    for e in exports.chain(opened).filter_map(Result::err) {
        note(&format!("error: {e}"));
        code = ExitCode::FAILURE;
    }
    status("");
    code
}

/// Reports on the package at `path`, or on every member if `path` is a workspace root. They go
/// to the store, or to `out`: a directory, or `-` for stdout.
fn export(path: &Path, out: Option<&Path>) -> Result<(), Error> {
    let manifest = match path.is_dir() {
        true => path.join("Cargo.toml"),
        false => path.to_path_buf(),
    };
    status("reading cargo metadata");
    let meta = metadata(&manifest)?;
    let same = |a: &Path, b: &Path| matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b);
    let workspace = same(&manifest, &meta.workspace_root.join("Cargo.toml"));
    let packages: Vec<_> = meta
        .packages
        .iter()
        .filter(|p| meta.workspace_members.contains(&p.id))
        .filter(|p| workspace || same(&manifest, &p.manifest_path))
        .collect();
    if packages.is_empty() {
        return Err(Error::NoPackage(manifest));
    }
    let mut reports = Vec::new();
    for (i, package) in packages.iter().enumerate() {
        status(&format!(
            "analyzing {} ({}/{})",
            package.name,
            i + 1,
            packages.len()
        ));
        let analysis = analyze(&[input(package, &meta)]);
        for e in &analysis.errors {
            note(&format!("warning: {}: {e}", package.name));
        }
        reports.push(analysis.report);
    }
    let Some(out) = out else {
        return save(&manifest, &reports);
    };
    for report in &reports {
        if out == Path::new("-") {
            let json = serde_json::to_string(report).map_err(Error::Encode)?;
            let _held = wiped();
            writeln!(io::stdout(), "{json}").map_err(|source| Error::Io {
                path: "<stdout>".into(),
                source,
            })?;
        } else {
            let json = serde_json::to_string_pretty(report).map_err(Error::Encode)?;
            if let Some(c) = report.crates.first() {
                write(&out.join(format!("{}.json", c.name)), &(json + "\n"))?;
            }
        }
    }
    Ok(())
}

/// The store: `$RUST_VIEW_HOME`, else `~/.rust-view`.
fn home() -> Result<PathBuf, Error> {
    match env::var_os("RUST_VIEW_HOME").filter(|home| !home.is_empty()) {
        Some(home) => Ok(home.into()),
        None => env::home_dir()
            .map(|home| home.join(".rust-view"))
            .ok_or(Error::NoHome),
    }
}

/// Writes `path`, creating its directory.
fn write(path: &Path, contents: &str) -> Result<(), Error> {
    path.parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|()| fs::write(path, contents))
        .map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
}

/// Keeps the project's reports in the store, in place of its last ones, and lists it in the index.
fn save(manifest: &Path, reports: &[Report]) -> Result<(), Error> {
    let home = home()?;
    let manifest = manifest
        .canonicalize()
        .unwrap_or_else(|_| manifest.to_path_buf());
    let dir = manifest.parent().unwrap_or(Path::new("."));
    let path = dir.display().to_string();
    let name = dir
        .file_name()
        .map_or("project".into(), |name| name.to_string_lossy());
    // The name for reading, and a hash of the path (FNV-1a) to tell same-named projects apart.
    let slug: String = name
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphanumeric() || "-_.".contains(c) => c,
            _ => '_',
        })
        .collect();
    let hash = path.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let id = format!("{slug}-{hash:08x}");
    let script = |id: &str| home.join("reports").join(format!("{id}.js"));

    let body = serde_json::to_string(reports).map_err(Error::Encode)?;
    let key = serde_json::to_string(&id).map_err(Error::Encode)?;
    write(&script(&id), &format!("rustView.report({key},{body})\n"))?;

    let index = home.join("index.js");
    let listed = |text: String| {
        let entries = text.trim().strip_prefix(INDEX[0])?.strip_suffix(INDEX[1])?;
        serde_json::from_str::<Vec<Entry>>(entries).ok()
    };
    let mut entries = match fs::read_to_string(&index) {
        Ok(text) => listed(text).unwrap_or_else(|| {
            note(&format!(
                "warning: {}: not an index; starting a new one",
                index.display()
            ));
            Vec::new()
        }),
        Err(_) => Vec::new(),
    };
    // Drop this project's last entry, and any whose script has been deleted.
    entries.retain(|e| e.id != id && script(&e.id).exists());
    entries.push(Entry {
        id,
        name: name.into_owned(),
        path,
        time: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as u64),
        version: report::VERSION,
        crates: reports
            .iter()
            .flat_map(|r| &r.crates)
            .map(|c| c.name.clone())
            .collect(),
    });
    let entries = serde_json::to_string(&entries).map_err(Error::Encode)?;
    write(&index, &format!("{}{entries}{}\n", INDEX[0], INDEX[1]))?;
    write(&home.join("index.html"), PAGE)
}

/// Writes the dashboard into the store and opens it in the browser.
fn launch() -> Result<(), Error> {
    let page = home()?.join("index.html");
    write(&page, PAGE)?;
    let (program, args): (OsString, &[&str]) = match env::var_os("BROWSER") {
        Some(browser) if !browser.is_empty() => (browser, &[]),
        _ if cfg!(target_os = "macos") => ("open".into(), &[]),
        _ if cfg!(windows) => ("cmd".into(), &["/C", "start", ""]),
        _ => ("xdg-open".into(), &[]),
    };
    let failed = |message: String| Error::Open {
        page: page.clone(),
        message,
    };
    let status = Command::new(&program)
        .args(args)
        .arg(&page)
        .status()
        .map_err(|e| failed(format!("{}: {e}", program.display())))?;
    match status.success() {
        true => Ok(()),
        false => Err(failed(format!("{}: {status}", program.display()))),
    }
}

/// `cargo metadata` with dependencies resolved, or without if they can't be (e.g. offline).
fn metadata(manifest: &Path) -> Result<Metadata, Error> {
    query(manifest, &[]).or_else(|e| {
        let meta = query(manifest, &["--no-deps"])?;
        note(&format!("warning: dependency graph omitted: {e}"));
        Ok(meta)
    })
}

fn query(manifest: &Path, flags: &[&str]) -> Result<Metadata, Error> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let failed = |message: String| Error::Cargo {
        manifest: manifest.to_path_buf(),
        message,
    };
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--manifest-path"])
        .arg(manifest)
        .args(flags)
        .output()
        .map_err(|e| failed(e.to_string()))?;
    if !output.status.success() {
        return Err(failed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(Error::Metadata)
}

/// Reads the package's sources. Unreadable files are skipped with a warning.
fn input(package: &Package, meta: &Metadata) -> CrateInput {
    let dir = package.manifest_path.parent().unwrap_or(Path::new("."));
    let base = match dir.starts_with(&meta.workspace_root) {
        true => meta.workspace_root.as_path(),
        false => dir,
    };
    let mut paths = Vec::new();
    walk(dir, &meta.target_directory, &mut paths);
    for target in &package.targets {
        if !paths.contains(&target.src_path) {
            paths.push(target.src_path.clone());
        }
    }
    let mut files = Vec::new();
    for path in paths {
        let Some(rel) = relative(base, &path) else {
            note(&format!(
                "warning: {}: {}",
                package.name,
                Error::Outside(path)
            ));
            continue;
        };
        match fs::read_to_string(&path) {
            Ok(text) => files.push(Source { path: rel, text }),
            Err(source) => note(&format!(
                "warning: {}: {}",
                package.name,
                Error::Io { path, source }
            )),
        }
    }
    CrateInput {
        name: package.name.clone(),
        version: package.version.clone(),
        path: relative(base, dir).unwrap_or_else(|| ".".into()),
        dependencies: package.dependencies.iter().map(dependency).collect(),
        packages: packages(package, meta),
        roots: package
            .targets
            .iter()
            .filter_map(|t| relative(base, &t.src_path))
            .collect(),
        files,
    }
}

/// `root` and everything it transitively depends on, by name then version. Empty if unresolved.
fn packages(root: &Package, meta: &Metadata) -> Vec<report::Package> {
    let Some(resolve) = &meta.resolve else {
        return Vec::new();
    };
    let by_id: HashMap<&str, &Package> = meta.packages.iter().map(|p| (p.id.as_str(), p)).collect();
    let nodes: HashMap<&str, &Node> = resolve.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut seen = HashSet::from([root.id.as_str()]);
    let mut queue = vec![root.id.as_str()];
    let mut out = Vec::new();
    while let Some(id) = queue.pop() {
        let (Some(package), Some(node)) = (by_id.get(id), nodes.get(id)) else {
            continue;
        };
        let mut dependencies = Vec::new();
        for dep in &node.deps {
            let Some(target) = by_id.get(dep.pkg.as_str()) else {
                continue;
            };
            if seen.insert(&dep.pkg) {
                queue.push(&dep.pkg);
            }
            // One entry per kind; a kind repeats when it is declared for several targets.
            let mut kinds = Vec::new();
            for kind in dep
                .dep_kinds
                .iter()
                .map(|k| k.kind.unwrap_or(DepKind::Normal))
            {
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                    dependencies.push(report::PackageDep {
                        name: target.name.clone(),
                        version: target.version.clone(),
                        kind,
                    });
                }
            }
        }
        out.push(report::Package {
            name: package.name.clone(),
            version: package.version.clone(),
            source: source(package.source.as_deref()),
            dependencies,
        });
    }
    out.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    out
}

fn source(source: Option<&str>) -> DepSource {
    match source {
        None => DepSource::Path,
        Some(s) if s.starts_with("git+") => DepSource::Git,
        Some(_) => DepSource::Registry,
    }
}

fn dependency(dep: &Dep) -> Dependency {
    let source = source(dep.source.as_deref());
    // cargo prints a bare `1.2` as `^1.2`, and no requirement as `*`.
    let req = match (dep.req.as_str(), source) {
        ("*", DepSource::Path | DepSource::Git) => None,
        (req, _) => Some(req.strip_prefix('^').unwrap_or(req).to_string()),
    };
    Dependency {
        name: dep.name.clone(),
        req,
        kind: dep.kind.unwrap_or(DepKind::Normal),
        source,
    }
}

/// Collects `.rs` files under `dir` in name order, skipping `target`, hidden directories, and
/// nested packages.
fn walk(dir: &Path, target: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => {
                let hidden = entry.file_name().to_string_lossy().starts_with('.');
                if !hidden && path != target && !path.join("Cargo.toml").exists() {
                    walk(&path, target, out);
                }
            }
            Ok(t) if t.is_file() && path.extension().is_some_and(|e| e == "rs") => out.push(path),
            _ => {}
        }
    }
}

/// `path` relative to `base`, `/`-separated; `.` for `base` itself.
fn relative(base: &Path, path: &Path) -> Option<String> {
    let rest = path.strip_prefix(base).ok()?;
    let parts: Vec<_> = rest.iter().map(|p| p.to_string_lossy()).collect();
    Some(match parts.is_empty() {
        true => ".".into(),
        false => parts.join("/"),
    })
}
