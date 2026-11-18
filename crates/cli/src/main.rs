//! `rust-view`: one JSON report per crate of the given cargo projects.

use rust_view_core::report::{self, DepKind, DepSource, Dependency};
use rust_view_core::{CrateInput, Source, analyze};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fmt, fs, io};

const USAGE: &str = "\
usage: rust-view [-i PATH]... [-o DIR] [PATH]...

Writes one JSON report per crate: one for a single crate, one per member for a workspace.
PATH is a cargo project directory or its Cargo.toml (default: .).

  -i, --input PATH  report on PATH; repeatable, same as a bare PATH
  -o, --out DIR     write DIR/<crate>.json instead of one report per line on stdout
  -h, --help        print this help
";

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

fn main() -> ExitCode {
    let (mut paths, mut out) = (Vec::new(), None);
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
            Some(flag) if flag.starts_with('-') => Error::Usage(format!("unknown option `{flag}`")),
            _ => {
                paths.push(PathBuf::from(arg));
                continue;
            }
        };
        eprintln!("error: {usage}\n\n{USAGE}");
        return ExitCode::from(2);
    }
    if paths.is_empty() {
        paths.push(".".into());
    }
    let mut code = ExitCode::SUCCESS;
    for path in &paths {
        if let Err(e) = export(path, out.as_deref()) {
            eprintln!("error: {e}");
            code = ExitCode::FAILURE;
        }
    }
    code
}

/// Reports on the package at `path`, or on every member if `path` is a workspace root.
fn export(path: &Path, out: Option<&Path>) -> Result<(), Error> {
    let manifest = match path.is_dir() {
        true => path.join("Cargo.toml"),
        false => path.to_path_buf(),
    };
    let meta = metadata(&manifest)?;
    let same = |a: &Path, b: &Path| matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b);
    let workspace = same(&manifest, &meta.workspace_root.join("Cargo.toml"));
    let mut packages = meta
        .packages
        .iter()
        .filter(|p| meta.workspace_members.contains(&p.id))
        .filter(|p| workspace || same(&manifest, &p.manifest_path))
        .peekable();
    if packages.peek().is_none() {
        return Err(Error::NoPackage(manifest));
    }
    for package in packages {
        let analysis = analyze(&[input(package, &meta)]);
        for e in &analysis.errors {
            eprintln!("warning: {}: {e}", package.name);
        }
        match out {
            Some(dir) => {
                let json = serde_json::to_string_pretty(&analysis.report).map_err(Error::Encode)?;
                let path = dir.join(format!("{}.json", package.name));
                fs::create_dir_all(dir)
                    .and_then(|()| fs::write(&path, json + "\n"))
                    .map_err(|source| Error::Io { path, source })?;
            }
            None => {
                let json = serde_json::to_string(&analysis.report).map_err(Error::Encode)?;
                writeln!(io::stdout(), "{json}").map_err(|source| Error::Io {
                    path: "<stdout>".into(),
                    source,
                })?;
            }
        }
    }
    Ok(())
}

/// `cargo metadata` with dependencies resolved, or without if they can't be (e.g. offline).
fn metadata(manifest: &Path) -> Result<Metadata, Error> {
    query(manifest, &[]).or_else(|e| {
        let meta = query(manifest, &["--no-deps"])?;
        eprintln!("warning: dependency graph omitted: {e}");
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
            eprintln!("warning: {}: {}", package.name, Error::Outside(path));
            continue;
        };
        match fs::read_to_string(&path) {
            Ok(text) => files.push(Source { path: rel, text }),
            Err(source) => eprintln!("warning: {}: {}", package.name, Error::Io { path, source }),
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
