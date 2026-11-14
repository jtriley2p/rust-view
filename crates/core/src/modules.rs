//! Module tree: which file defines which module.

use crate::scan::Scan;
use crate::{CrateInput, Error};
use std::collections::{HashMap, HashSet};

/// Where a file sits in its crate.
#[derive(Debug)]
pub struct Place {
    /// Index into `CrateInput::roots` of the target the file belongs to.
    pub root: usize,
    /// Module path, e.g. `crate::a::b`.
    pub module: String,
    /// Under `tests/`, or declared by a test-only `mod`.
    pub test: bool,
}

#[derive(Debug, Default)]
pub struct Tree {
    pub files: HashMap<String, Place>,
    /// (root, module path) -> defining file, inline modules included.
    pub modules: HashMap<(usize, String), String>,
}

pub fn build(krate: &CrateInput, scans: &HashMap<&str, Scan>, errors: &mut Vec<Error>) -> Tree {
    let mut tree = Tree::default();
    let tests_dir = join(&[&krate.path, "tests"]) + "/";
    for (root, path) in krate.roots.iter().enumerate() {
        // (file, module, whether child modules sit beside the file, test)
        let mut stack = vec![(path.clone(), "crate".to_string(), true, false)];
        let mut seen = HashSet::new();
        while let Some((path, module, owns_dir, test)) = stack.pop() {
            let Some(scan) = scans.get(path.as_str()) else {
                continue;
            };
            if !seen.insert(path.clone()) {
                continue;
            }
            let test = test || path.starts_with(&tests_dir);
            let (dir, file) = path.rsplit_once('/').unwrap_or(("", &path));
            let base = match owns_dir {
                true => dir.to_string(),
                false => join(&[dir, file.strip_suffix(".rs").unwrap_or(file)]),
            };
            for m in &scan.mods {
                let child = format!("{}::{}", scoped(&module, &m.scope), m.name);
                if m.inline {
                    if !m.test {
                        tree.modules.insert((root, child), path.clone());
                    }
                    continue;
                }
                let inner = join(&[&base, &m.scope.join("/")]);
                let candidates = match &m.path {
                    Some(p) if m.scope.is_empty() => vec![(join(&[dir, p]), true)],
                    Some(p) => vec![(join(&[&inner, p]), true)],
                    None => vec![
                        (join(&[&inner, &format!("{}.rs", m.name)]), false),
                        (join(&[&inner, &m.name, "mod.rs"]), true),
                    ],
                };
                match candidates
                    .into_iter()
                    .find(|(p, _)| scans.contains_key(p.as_str()))
                {
                    Some((file, owns_dir)) => stack.push((file, child, owns_dir, test || m.test)),
                    None => errors.push(Error::MissingModule {
                        path: path.clone(),
                        module: child,
                    }),
                }
            }
            tree.modules.insert((root, module.clone()), path.clone());
            tree.files
                .entry(path)
                .or_insert(Place { root, module, test });
        }
    }
    for f in &krate.files {
        if scans.contains_key(f.path.as_str()) && !tree.files.contains_key(&f.path) {
            errors.push(Error::Orphan {
                path: f.path.clone(),
            });
        }
    }
    tree
}

/// `module` extended by the inline modules in `scope`.
pub fn scoped(module: &str, scope: &[String]) -> String {
    scope.iter().fold(module.to_string(), |m, s| m + "::" + s)
}

/// Joins `/`-separated paths, resolving `.` and `..`.
fn join(parts: &[&str]) -> String {
    let mut out = Vec::new();
    for seg in parts.iter().flat_map(|p| p.split('/')) {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            seg => out.push(seg),
        }
    }
    out.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Source;

    fn tree(path: &str, roots: &[&str], files: &[(&str, &str)]) -> (Tree, Vec<Error>) {
        let krate = CrateInput {
            name: "k".into(),
            path: path.into(),
            dependencies: vec![],
            roots: roots.iter().map(|r| r.to_string()).collect(),
            files: files
                .iter()
                .map(|(p, t)| Source {
                    path: p.to_string(),
                    text: t.to_string(),
                })
                .collect(),
        };
        let scans = files
            .iter()
            .map(|(p, t)| (*p, crate::scan::scan(t).unwrap()))
            .collect();
        let mut errors = vec![];
        (build(&krate, &scans, &mut errors), errors)
    }

    #[test]
    fn layouts() {
        let (t, errors) = tree(
            "k",
            &["k/src/lib.rs", "k/tests/it.rs"],
            &[
                (
                    "k/src/lib.rs",
                    "mod a; mod b; mod inline { mod c; } #[path = \"other/p.rs\"] mod p; #[cfg(test)] mod t; mod gone;",
                ),
                ("k/src/a.rs", "mod x;"),
                ("k/src/a/x.rs", ""),
                ("k/src/b/mod.rs", "mod y;"),
                ("k/src/b/y.rs", ""),
                ("k/src/inline/c.rs", ""),
                ("k/src/other/p.rs", "mod q;"),
                ("k/src/other/q.rs", ""),
                ("k/src/t.rs", "mod deep;"),
                ("k/src/t/deep.rs", ""),
                ("k/tests/it.rs", "mod common;"),
                ("k/tests/common/mod.rs", ""),
                ("k/src/stray.rs", ""),
            ],
        );
        let module = |p: &str| (t.files[p].root, t.files[p].module.as_str(), t.files[p].test);
        assert_eq!(module("k/src/lib.rs"), (0, "crate", false));
        assert_eq!(module("k/src/a/x.rs"), (0, "crate::a::x", false));
        assert_eq!(module("k/src/b/y.rs"), (0, "crate::b::y", false));
        assert_eq!(module("k/src/inline/c.rs"), (0, "crate::inline::c", false));
        assert_eq!(module("k/src/other/q.rs"), (0, "crate::p::q", false));
        assert_eq!(module("k/src/t/deep.rs"), (0, "crate::t::deep", true));
        assert_eq!(module("k/tests/it.rs"), (1, "crate", true));
        assert_eq!(module("k/tests/common/mod.rs"), (1, "crate::common", true));
        assert_eq!(t.modules[&(0, "crate::inline".to_string())], "k/src/lib.rs");
        assert_eq!(
            errors,
            [
                Error::MissingModule {
                    path: "k/src/lib.rs".into(),
                    module: "crate::gone".into()
                },
                Error::Orphan {
                    path: "k/src/stray.rs".into()
                },
            ]
        );
    }

    #[test]
    fn root_package() {
        let (t, errors) = tree(
            ".",
            &["src/main.rs", "tests/t.rs"],
            &[("src/main.rs", ""), ("tests/t.rs", "")],
        );
        assert!(!t.files["src/main.rs"].test && t.files["tests/t.rs"].test);
        assert_eq!(errors, []);
    }
}
