//! The report types, the analyzer, and `spec/` agree.

use rust_view_core::report::*;
use rust_view_core::{CrateInput, Error, Source, analyze};
use serde_json::{Value, json};

fn assert_valid(instance: &Value) {
    let schema = serde_json::from_str(include_str!("../spec/report.schema.json")).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("{}: {e}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn example_matches_schema_and_types() {
    let example: Value = serde_json::from_str(include_str!("../spec/example.json")).unwrap();
    assert_valid(&example);
    let report: Report = serde_json::from_value(example.clone()).unwrap();
    assert_eq!(serde_json::to_value(&report).unwrap(), example);
}

const LIB: &str = "\
//! Key-value store.
mod error;
mod store;
mod wal;

pub use error::Error;
pub use serde::Serialize;
pub use store::{Entry, Store};

pub type Result<T> = std::result::Result<T, Error>;
";

const ERROR: &str = "\
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Corrupt,
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
";

const STORE: &str = "\
use crate::error::Error;
use crate::wal::Wal;
pub use crate::wal::Entry;

pub struct Store {
    wal: Wal,
}

impl Store {
    /// Latest entry for `key`.
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.wal.entries().iter().rev().find(|e| e.key == key)
    }

    pub fn put(&mut self, key: &str) -> Result<(), Error> {
        self.wal.append(key)?;
        Ok(())
    }

    pub fn each(&self, f: impl Fn(&Entry)) {
        self.wal.entries().iter().for_each(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_then_get() {
        let mut s = Store { wal: Wal::default() };
        s.put(\"a\").unwrap();
        assert!(s.get(\"a\").is_some());
    }

    #[test]
    #[ignore]
    fn slow() {}
}
";

const WAL: &str = "\
use crate::error::Error;

pub struct Entry {
    pub key: String,
}

#[derive(Default)]
pub struct Wal {
    entries: Vec<Entry>,
}

impl Wal {
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn append(&mut self, key: &str) -> Result<(), Error> {
        if key.is_empty() {
            return Err(Error::Corrupt);
        }
        self.entries.push(Entry { key: key.to_string() });
        Ok(())
    }

    pub fn compact(&mut self) {
        todo!()
    }

    pub fn first(&self) -> &Entry {
        self.entries.first().unwrap()
    }
}
";

const ROUNDTRIP: &str = "\
use kv_core::Store;

#[test]
fn roundtrip() {
    let s: Option<Store> = None;
    assert!(s.map(|_| 1).unwrap_or(0) == 0);
}
";

#[test]
fn analyzes_a_crate() {
    let source = |path: &str, text: &str| Source {
        path: format!("crates/core/{path}"),
        text: text.into(),
    };
    let dependencies = vec![Dependency {
        name: "serde".into(),
        req: Some("1".into()),
        kind: DepKind::Normal,
        source: DepSource::Registry,
    }];
    let packages = vec![
        Package {
            name: "kv-core".into(),
            version: "0.1.0".into(),
            source: DepSource::Path,
            dependencies: vec![PackageDep {
                name: "serde".into(),
                version: "1.0.229".into(),
                kind: DepKind::Normal,
            }],
        },
        Package {
            name: "serde".into(),
            version: "1.0.229".into(),
            source: DepSource::Registry,
            dependencies: vec![],
        },
    ];
    let analysis = analyze(&[CrateInput {
        name: "kv-core".into(),
        version: "0.1.0".into(),
        path: "crates/core".into(),
        dependencies: dependencies.clone(),
        packages: packages.clone(),
        roots: vec![
            "crates/core/src/lib.rs".into(),
            "crates/core/tests/roundtrip.rs".into(),
        ],
        files: vec![
            source("src/bad.rs", "fn ok() {}\nfn 1() {}\n"),
            source("src/error.rs", ERROR),
            source("src/lib.rs", LIB),
            source("src/store.rs", STORE),
            source("src/wal.rs", WAL),
            source("tests/roundtrip.rs", ROUNDTRIP),
        ],
    }]);

    assert!(
        matches!(&analysis.errors[..], [Error::Parse { path, line: 2, .. }] if path == "crates/core/src/bad.rs")
    );
    let report = analysis.report;
    let json = serde_json::to_value(&report).unwrap();
    assert_valid(&json);

    let krate = &report.crates[0];
    assert_eq!(krate.dependencies, dependencies);
    assert_eq!(
        (krate.version.as_str(), &krate.packages),
        ("0.1.0", &packages)
    );
    let [error, lib, store, wal, roundtrip] = &krate.files[..] else {
        panic!("{:#?}", krate.files)
    };

    assert_eq!(
        json["crates"][0]["files"][1],
        json!({
            "path": "crates/core/src/lib.rs",
            "module": "crate",
            "root": "crates/core/src/lib.rs",
            "lines": { "code": 7, "test": 0, "doc": 1, "comment": 0, "blank": 2 },
            "tests": { "count": 0, "ignored": 0 },
            "items": {
                "struct": 0, "enum": 0, "union": 0, "trait": 0, "impl": 0, "fn": 0, "method": 0,
                "const": 0, "static": 0, "type": 1, "mod": 3, "macro": 0
            },
            "functional": { "closures": 0, "combinators": 0, "higher_order_fns": 0 },
            "suppressions": { "todo": 0, "unimplemented": 0, "unreachable": 0, "allow": 0 },
            "error_handling": { "unwrap": 0, "expect": 0, "panic": 0, "propagate": 0 },
            "definitions": [{ "name": "Result", "kind": "type", "module": "crate", "public": true }],
            "imports": [
                { "module": "crate::error", "file": "crates/core/src/error.rs", "names": ["Error"] },
                { "module": "crate::store", "file": "crates/core/src/store.rs", "names": ["Entry", "Store"] }
            ],
            "reexports": [
                {
                    "name": "Error",
                    "trace": [{ "module": "crate::error", "file": "crates/core/src/error.rs", "name": "Error" }],
                    "end": "definition"
                },
                {
                    "name": "Serialize",
                    "trace": [{ "module": "serde", "file": null, "name": "Serialize" }],
                    "end": "external"
                },
                {
                    "name": "Entry",
                    "trace": [
                        { "module": "crate::store", "file": "crates/core/src/store.rs", "name": "Entry" },
                        { "module": "crate::wal", "file": "crates/core/src/wal.rs", "name": "Entry" }
                    ],
                    "end": "definition"
                },
                {
                    "name": "Store",
                    "trace": [{ "module": "crate::store", "file": "crates/core/src/store.rs", "name": "Store" }],
                    "end": "definition"
                }
            ]
        })
    );
    assert_eq!(lib.module, "crate");

    assert_eq!(
        error.metrics.items,
        Items {
            r#enum: 1,
            r#impl: 1,
            method: 1,
            ..Default::default()
        }
    );
    assert!(error.imports.is_empty() && error.reexports.is_empty());

    let m = store.metrics;
    assert_eq!(
        m.lines,
        Lines {
            code: 18,
            test: 13,
            doc: 1,
            comment: 0,
            blank: 7
        }
    );
    assert_eq!(
        m.tests,
        Tests {
            count: 2,
            ignored: 1
        }
    );
    assert_eq!(
        m.items,
        Items {
            r#struct: 1,
            r#impl: 1,
            method: 3,
            ..Default::default()
        }
    );
    assert_eq!(
        m.functional,
        Functional {
            closures: 1,
            combinators: 3,
            higher_order_fns: 1
        }
    );
    assert_eq!(
        m.error_handling,
        ErrorHandling {
            propagate: 1,
            ..Default::default()
        }
    );
    let imports: Vec<_> = store
        .imports
        .iter()
        .map(|i| (i.module.as_str(), i.names.join(",")))
        .collect();
    assert_eq!(
        imports,
        [
            ("crate::error", "Error".to_string()),
            ("crate::wal", "Entry,Wal".to_string())
        ]
    );
    assert_eq!(store.reexports.len(), 1);
    // Methods and test code are not definitions.
    let definitions: Vec<_> = store
        .definitions
        .iter()
        .map(|d| (d.name.as_str(), d.kind, d.module.as_str(), d.public))
        .collect();
    assert_eq!(
        definitions,
        [("Store", ItemKind::Struct, "crate::store", true)]
    );

    assert_eq!(
        wal.metrics.suppressions,
        Suppressions {
            todo: 1,
            ..Default::default()
        }
    );
    assert_eq!(
        wal.metrics.error_handling,
        ErrorHandling {
            unwrap: 1,
            ..Default::default()
        }
    );

    assert_eq!(roundtrip.module, "crate");
    assert_eq!(
        roundtrip.metrics,
        Metrics {
            lines: Lines {
                code: 0,
                test: 6,
                doc: 0,
                comment: 0,
                blank: 1
            },
            tests: Tests {
                count: 1,
                ignored: 0
            },
            ..Default::default()
        }
    );
    assert!(roundtrip.imports.is_empty() && roundtrip.definitions.is_empty());
    assert_eq!(
        (store.root.as_str(), roundtrip.root.as_str()),
        ("crates/core/src/lib.rs", "crates/core/tests/roundtrip.rs")
    );

    let mut sum = Totals::default();
    krate.files.iter().for_each(|f| {
        sum += Totals {
            files: 1,
            metrics: f.metrics,
        }
    });
    assert_eq!(krate.totals, sum);
    assert_eq!(report.totals, sum);
    assert_eq!((sum.files, sum.metrics.tests.count), (5, 3));
}

#[test]
fn schema_rejects_drift() {
    let mut example: Value = serde_json::from_str(include_str!("../spec/example.json")).unwrap();
    example["crates"][0]["files"][0]["dead_items"] = json!(0);
    let schema = serde_json::from_str(include_str!("../spec/report.schema.json")).unwrap();
    assert!(
        !jsonschema::validator_for(&schema)
            .unwrap()
            .is_valid(&example)
    );
}
