//! Static metrics for Rust source. Pure: takes source text, returns a [`Report`].

mod flow;
mod lines;
mod modules;
pub mod report;
mod resolve;
mod scan;

pub use report::Report;

use report::{Crate, Definition, Dependency, File, Metrics, Package, Totals};
use std::collections::HashMap;
use std::fmt;

/// One package. Paths are workspace-relative and `/`-separated.
#[derive(Debug, Clone)]
pub struct CrateInput {
    pub name: String,
    pub version: String,
    /// Package directory; `.` for a root package.
    pub path: String,
    pub dependencies: Vec<Dependency>,
    /// Resolved dependency graph: the crate and all it transitively depends on. Empty if unresolved.
    pub packages: Vec<Package>,
    /// Root file of each target: `src/lib.rs`, `src/main.rs`, `tests/*.rs`, ..
    pub roots: Vec<String>,
    pub files: Vec<Source>,
}

#[derive(Debug, Clone)]
pub struct Source {
    pub path: String,
    pub text: String,
}

#[derive(Debug)]
pub struct Analysis {
    pub report: Report,
    /// Files skipped and modules not found; the report covers the rest.
    pub errors: Vec<Error>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// The file is not valid Rust.
    Parse {
        path: String,
        line: usize,
        message: String,
    },
    /// `path` declares `mod` `module`, but no file for it was given.
    MissingModule { path: String, module: String },
    /// No target root reaches the file through `mod` declarations.
    Orphan { path: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Parse {
                path,
                line,
                message,
            } => write!(f, "{path}:{line}: {message}"),
            Error::MissingModule { path, module } => {
                write!(f, "{path}: no file for module `{module}`")
            }
            Error::Orphan { path } => write!(f, "{path}: not reachable from any target root"),
        }
    }
}

impl std::error::Error for Error {}

pub fn analyze(crates: &[CrateInput]) -> Analysis {
    let mut errors = Vec::new();
    let mut report = Report {
        version: report::VERSION,
        totals: Totals::default(),
        crates: Vec::new(),
    };
    for krate in crates {
        let mut scans = HashMap::new();
        for f in &krate.files {
            match scan::scan(&f.text) {
                Ok(scan) => {
                    scans.insert(f.path.as_str(), scan);
                }
                Err(e) => errors.push(Error::Parse {
                    path: f.path.clone(),
                    line: e.span().start().line,
                    message: e.to_string(),
                }),
            }
        }
        let tree = modules::build(krate, &scans, &mut errors);
        let index = resolve::Index::new(&tree, &scans);

        let mut totals = Totals::default();
        let mut files = Vec::new();
        for f in &krate.files {
            let (Some(scan), Some(place)) = (scans.get(f.path.as_str()), tree.files.get(&f.path))
            else {
                continue;
            };
            let mut metrics = scan.metrics;
            let (mut definitions, mut imports, mut reexports, mut flows) =
                (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            if place.test {
                // The whole file is test code.
                metrics = Metrics {
                    lines: metrics.lines,
                    tests: metrics.tests,
                    ..Metrics::default()
                };
                metrics.lines.test += std::mem::take(&mut metrics.lines.code);
            } else {
                definitions = scan
                    .defs
                    .iter()
                    .filter_map(|d| {
                        Some(Definition {
                            name: d.name.clone(),
                            kind: d.kind?,
                            module: modules::scoped(&place.module, &d.scope),
                            public: d.public,
                        })
                    })
                    .collect();
                imports = index.imports(&f.path, place, scan);
                reexports = index.reexports(place, scan);
                flows = flow::flows(&index, &f.path, place, scan);
            }
            totals += Totals { files: 1, metrics };
            files.push(File {
                path: f.path.clone(),
                module: place.module.clone(),
                root: krate.roots.get(place.root).cloned().unwrap_or_default(),
                metrics,
                definitions,
                imports,
                reexports,
                flows,
            });
        }
        report.totals += totals;
        report.crates.push(Crate {
            name: krate.name.clone(),
            version: krate.version.clone(),
            path: krate.path.clone(),
            totals,
            dependencies: krate.dependencies.clone(),
            packages: krate.packages.clone(),
            files,
        });
    }
    Analysis { report, errors }
}
