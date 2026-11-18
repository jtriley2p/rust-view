//! Import edges and re-export traces.

use crate::modules::{Place, Tree, scoped};
use crate::report::{End, Hop, Import, ItemKind, Reexport};
use crate::scan::{Alias, Scan, Use};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// What each non-test module defines and re-exports.
pub struct Index<'a> {
    tree: &'a Tree,
    modules: HashMap<(usize, String), Module<'a>>,
}

#[derive(Default)]
struct Module<'a> {
    defs: HashSet<&'a str>,
    pubs: Vec<&'a Use>,
    /// `use` declarations, private ones included.
    uses: Vec<&'a Use>,
    /// Structs, enums, and unions.
    types: HashSet<&'a str>,
    aliases: HashMap<&'a str, &'a Alias>,
}

/// What a type path names.
pub enum Named<'a> {
    /// A struct, enum, or union of the crate.
    Type(Hop),
    /// A `type` alias, and the module defining it.
    Alias(String, &'a Alias),
    External(Hop),
}

impl<'a> Index<'a> {
    pub fn new(tree: &'a Tree, scans: &'a HashMap<&str, Scan>) -> Self {
        let mut modules: HashMap<_, Module> = HashMap::new();
        for (path, place) in tree.files.iter().filter(|(_, p)| !p.test) {
            let Some(scan) = scans.get(path.as_str()) else {
                continue;
            };
            let key = |scope| (place.root, scoped(&place.module, scope));
            for d in &scan.defs {
                let module = modules.entry(key(&d.scope)).or_default();
                module.defs.insert(&d.name);
                if let Some(ItemKind::Struct | ItemKind::Enum | ItemKind::Union) = d.kind {
                    module.types.insert(&d.name);
                }
            }
            for u in scan.uses.iter().filter(|u| u.declared) {
                let module = modules.entry(key(&u.scope)).or_default();
                module.uses.push(u);
                if u.public {
                    module.pubs.push(u);
                }
            }
            for a in &scan.aliases {
                let module = modules.entry(key(&a.scope)).or_default();
                module.aliases.insert(&a.name, a);
            }
        }
        Index { tree, modules }
    }

    /// One edge per module the file imports from, sorted by module.
    pub fn imports(&self, path: &str, place: &Place, scan: &Scan) -> Vec<Import> {
        let mut out = BTreeMap::new();
        for u in &scan.uses {
            let from = scoped(&place.module, &u.scope);
            let Some((module, file, rest)) = self.resolve(place.root, &from, &u.path) else {
                continue;
            };
            if file == path {
                continue;
            }
            let (_, names) = out
                .entry(module)
                .or_insert_with(|| (file.to_string(), BTreeSet::new()));
            match rest.first() {
                Some(name) => names.insert(name.clone()),
                None => u.name == "*" && names.insert("*".to_string()),
            };
        }
        out.into_iter()
            .map(|(module, (file, names))| Import {
                module,
                file,
                names: names.into_iter().collect(),
            })
            .collect()
    }

    /// The file's `pub use` names, each traced to its definition.
    pub fn reexports(&self, place: &Place, scan: &Scan) -> Vec<Reexport> {
        let exported = scan.uses.iter().filter(|u| u.public && u.name != "_");
        exported
            .map(|u| {
                let (trace, end) = self.trace(place.root, &scoped(&place.module, &u.scope), u);
                Reexport {
                    name: u.name.clone(),
                    trace,
                    end,
                }
            })
            .collect()
    }

    /// The type `path` names where it is written in module `from`. None for a name nothing in
    /// the crate brings into scope: a primitive, a prelude type, a type parameter.
    pub fn named(&self, root: usize, from: &str, path: &[String]) -> Option<Named<'a>> {
        let here = self.modules.get(&(root, from.to_string()));
        let (first, name) = (path.first()?, path.last()?);
        let mut full = path.to_vec();
        let mut globbed = None;
        let local = matches!(first.as_str(), "crate" | "self" | "super")
            || here.is_some_and(|m| m.defs.contains(first.as_str()));
        if !local {
            let uses = here.map(|m| &m.uses[..]).unwrap_or_default();
            if let Some(u) = uses.iter().find(|u| u.name == *first) {
                full.splice(..1, u.path.iter().cloned());
            } else if path.len() == 1 {
                // Only a glob can bring the name in.
                let globs = uses.iter().filter(|u| u.name == "*");
                let mut hops = globs.filter_map(|u| self.hop(root, from, u));
                let hop =
                    hops.find(|h| h.name == "*" && self.provides(root, &h.module, name, 8))?;
                globbed = Some(Hop {
                    name: name.clone(),
                    ..hop
                });
            }
        }
        // A lone segment from outside is a crate, not a type.
        let lone = full.len() == 1;
        let (trace, end) = match globbed {
            Some(hop) => self.follow(root, hop),
            None => {
                let named = Use {
                    scope: Vec::new(),
                    name: name.clone(),
                    path: full,
                    public: false,
                    declared: false,
                };
                self.trace(root, from, &named)
            }
        };
        let hop = trace.into_iter().last()?;
        match end {
            End::Definition => {
                let module = self.modules.get(&(root, hop.module.clone()))?;
                match module.aliases.get(hop.name.as_str()) {
                    Some(alias) => Some(Named::Alias(hop.module, alias)),
                    None => module
                        .types
                        .contains(hop.name.as_str())
                        .then_some(Named::Type(hop)),
                }
            }
            End::External => (!lone).then_some(Named::External(hop)),
            End::Unresolved => None,
        }
    }

    fn trace(&self, root: usize, from: &str, u: &Use) -> (Vec<Hop>, End) {
        match self.hop(root, from, u) {
            Some(hop) => self.follow(root, hop),
            None => (vec![external(u)], End::External),
        }
    }

    /// Follows `hop` through re-exports to its definition.
    fn follow(&self, root: usize, mut hop: Hop) -> (Vec<Hop>, End) {
        let mut trace = Vec::new();
        loop {
            if trace.contains(&hop) {
                return (trace, End::Unresolved);
            }
            trace.push(hop.clone());
            // `Enum::Variant` is defined where `Enum` is.
            let name = hop.name.split("::").next().unwrap_or_default().to_string();
            if name == "*" {
                return (trace, End::Definition);
            }
            let Some(module) = self.modules.get(&(root, hop.module.clone())) else {
                return (trace, End::Unresolved);
            };
            if module.defs.contains(name.as_str()) {
                return (trace, End::Definition);
            }
            // Re-exported again: by name, else through a glob.
            if let Some(next) = module.pubs.iter().find(|p| p.name == name) {
                match self.hop(root, &hop.module, next) {
                    Some(next) => hop = next,
                    None => {
                        return (
                            trace.into_iter().chain([external(next)]).collect(),
                            End::External,
                        );
                    }
                }
                continue;
            }
            let globs = self.globbed(root, &hop.module);
            match globs
                .into_iter()
                .find(|g| self.provides(root, &g.module, &name, 8))
            {
                Some(next) => hop = Hop { name, ..next },
                None => return (trace, End::Unresolved),
            }
        }
    }

    /// The module and name a `pub use` takes from; None if outside the crate.
    fn hop(&self, root: usize, from: &str, u: &Use) -> Option<Hop> {
        let (mut module, mut file, rest) = self.resolve(root, from, &u.path)?;
        let mut name = rest.join("::");
        if u.name == "*" {
            name += if name.is_empty() { "*" } else { "::*" };
        } else if name.is_empty() {
            // A module is re-exported; its parent defines it.
            let (parent, last) = module.rsplit_once("::")?;
            name = last.to_string();
            file = self.tree.modules.get(&(root, parent.to_string()))?;
            module = parent.to_string();
        }
        Some(Hop {
            module,
            file: Some(file.to_string()),
            name,
        })
    }

    /// Modules that `module` glob re-exports.
    fn globbed(&self, root: usize, module: &str) -> Vec<Hop> {
        let pubs = self
            .modules
            .get(&(root, module.to_string()))
            .map(|m| &m.pubs[..])
            .unwrap_or_default();
        pubs.iter()
            .filter_map(|p| self.hop(root, module, p))
            .filter(|h| h.name == "*")
            .collect()
    }

    /// Whether `name` is defined in or re-exported by `module`, following globs up to `depth`.
    fn provides(&self, root: usize, module: &str, name: &str, depth: u8) -> bool {
        let Some(m) = self.modules.get(&(root, module.to_string())) else {
            return false;
        };
        m.defs.contains(name)
            || m.pubs.iter().any(|p| p.name == name)
            || depth > 0
                && self
                    .globbed(root, module)
                    .iter()
                    .any(|g| self.provides(root, &g.module, name, depth - 1))
    }

    /// Resolves `path`, written in module `from`, to the deepest module it
    /// names, that module's file, and the segments left over. None if the
    /// path leaves the crate.
    fn resolve<'p>(
        &self,
        root: usize,
        from: &str,
        path: &'p [String],
    ) -> Option<(String, &str, &'p [String])> {
        let mut base: Vec<&str> = from.split("::").collect();
        let mut rest = path;
        match path.first()?.as_str() {
            "crate" => {
                base.truncate(1);
                rest = &path[1..];
            }
            "self" => rest = &path[1..],
            "super" => {
                while rest.first().is_some_and(|s| s == "super") {
                    base.pop();
                    rest = &rest[1..];
                }
            }
            // Otherwise the path is relative only if `from` defines its first segment.
            first => {
                if !self
                    .modules
                    .get(&(root, from.to_string()))?
                    .defs
                    .contains(first)
                {
                    return None;
                }
            }
        }
        if base.is_empty() {
            return None;
        }
        // Longest prefix of `base + rest` that is a module.
        (0..=rest.len()).rev().find_map(|n| {
            let module = base
                .iter()
                .copied()
                .chain(rest[..n].iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join("::");
            let file = self.tree.modules.get(&(root, module.clone()))?;
            Some((module, file.as_str(), &rest[n..]))
        })
    }
}

/// The hop for a path outside the crate.
fn external(u: &Use) -> Hop {
    let (module, name) = match u.path.split_last() {
        _ if u.name == "*" => (&u.path[..], "*"),
        Some((name, module)) if !module.is_empty() => (module, name.as_str()),
        _ => (&u.path[..], u.path.first().map_or("", String::as_str)),
    };
    Hop {
        module: module.join("::"),
        file: None,
        name: name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::report::End;
    use crate::{CrateInput, Source, analyze};

    const LIB: &str = "
        mod a;
        mod b;
        pub mod shapes { pub enum Kind { X } pub fn area() {} }
        pub use a::*;
        pub use a::Deep as Renamed;
        pub use a::Missing;
        pub use shapes::Kind::*;
        pub use shapes::Kind::X;
        pub use self::b as bee;
        pub use a::Loop;
        pub use std::collections::*;
        pub use a::Ext;
        pub(crate) use a::own as _;
        fn f() { shapes::area(); crate::b::Deep; }
    ";
    const A: &str = "
        pub use crate::b::*;
        pub use crate::b::Loop;
        pub use serde::de::Error as Ext;
        pub fn own() {}
        use super::shapes::{self, Kind};
    ";
    const B: &str = "
        pub struct Deep;
        pub use crate::a::Loop;
    ";

    #[test]
    fn macro_wrapped_items() {
        let source = |path: &str, text: &str| Source {
            path: path.into(),
            text: text.into(),
        };
        let analysis = analyze(&[CrateInput {
            name: "k".into(),
            version: "0.1.0".into(),
            path: ".".into(),
            dependencies: vec![],
            packages: vec![],
            roots: vec!["src/lib.rs".into()],
            files: vec![
                source("src/lib.rs", "wrap! { mod a; } pub use a::{A, B, Built};"),
                source(
                    "src/a.rs",
                    r#"wrap! { pub struct A; } keywords! { "b" pub struct B } build!(Built);"#,
                ),
            ],
        }]);
        assert_eq!(analysis.errors, []);
        let ends: Vec<_> = analysis.report.crates[0].files[0]
            .reexports
            .iter()
            .map(|r| (r.name.as_str(), r.end))
            .collect();
        // A name the macro builds from its arguments stays out of reach.
        assert_eq!(
            ends,
            [
                ("A", End::Definition),
                ("B", End::Definition),
                ("Built", End::Unresolved)
            ]
        );
    }

    #[test]
    fn imports_and_reexports() {
        let source = |path: &str, text: &str| Source {
            path: path.into(),
            text: text.into(),
        };
        let analysis = analyze(&[CrateInput {
            name: "k".into(),
            version: "0.1.0".into(),
            path: ".".into(),
            dependencies: vec![],
            packages: vec![],
            roots: vec!["src/lib.rs".into()],
            files: vec![
                source("src/lib.rs", LIB),
                source("src/a.rs", A),
                source("src/b.rs", B),
            ],
        }]);
        assert_eq!(analysis.errors, []);
        let [lib, a, b] = &analysis.report.crates[0].files[..] else {
            panic!()
        };

        let imports = |f: &crate::report::File| -> Vec<String> {
            f.imports
                .iter()
                .map(|i| format!("{} {} {:?}", i.module, i.file, i.names))
                .collect()
        };
        assert_eq!(
            imports(lib),
            [
                r#"crate::a src/a.rs ["*", "Deep", "Ext", "Loop", "Missing", "own"]"#,
                r#"crate::b src/b.rs ["Deep"]"#,
            ]
        );
        assert_eq!(
            imports(a),
            [
                r#"crate::b src/b.rs ["*", "Loop"]"#,
                r#"crate::shapes src/lib.rs ["Kind"]"#
            ]
        );
        assert_eq!(imports(b), [r#"crate::a src/a.rs ["Loop"]"#]);

        let reexports = |f: &crate::report::File| -> Vec<String> {
            let hop = |h: &crate::report::Hop| {
                format!(
                    "{}::{} in {}",
                    h.module,
                    h.name,
                    h.file.as_deref().unwrap_or("-")
                )
            };
            let trace = |r: &crate::report::Reexport| {
                r.trace.iter().map(hop).collect::<Vec<_>>().join(" > ")
            };
            f.reexports
                .iter()
                .map(|r| format!("{} = {} ({:?})", r.name, trace(r), r.end))
                .collect()
        };
        assert_eq!(
            reexports(lib),
            [
                "* = crate::a::* in src/a.rs (Definition)",
                "Renamed = crate::a::Deep in src/a.rs > crate::b::Deep in src/b.rs (Definition)",
                "Missing = crate::a::Missing in src/a.rs (Unresolved)",
                "* = crate::shapes::Kind::* in src/lib.rs (Definition)",
                "X = crate::shapes::Kind::X in src/lib.rs (Definition)",
                "bee = crate::b in src/lib.rs (Definition)",
                "Loop = crate::a::Loop in src/a.rs > crate::b::Loop in src/b.rs (Unresolved)",
                "* = std::collections::* in - (External)",
                "Ext = crate::a::Ext in src/a.rs > serde::de::Error in - (External)",
            ]
        );
        assert_eq!(
            reexports(a),
            [
                "* = crate::b::* in src/b.rs (Definition)",
                "Loop = crate::b::Loop in src/b.rs > crate::a::Loop in src/a.rs (Unresolved)",
                "Ext = serde::de::Error in - (External)",
            ]
        );
    }
}
