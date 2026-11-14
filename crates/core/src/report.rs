//! Report types; the serialized form is specified by `spec/report.schema.json`.

use serde::{Deserialize, Serialize};
use std::ops::AddAssign;

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub version: u32,
    pub totals: Totals,
    pub crates: Vec<Crate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Crate {
    pub name: String,
    pub path: String,
    pub totals: Totals,
    pub dependencies: Vec<Dependency>,
    pub files: Vec<File>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub req: Option<String>,
    pub kind: DepKind,
    pub source: DepSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DepKind {
    Normal,
    Dev,
    Build,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DepSource {
    Registry,
    Path,
    Git,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    pub files: u32,
    #[serde(flatten)]
    pub metrics: Metrics,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub module: String,
    #[serde(flatten)]
    pub metrics: Metrics,
    pub imports: Vec<Import>,
    pub reexports: Vec<Reexport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Import {
    pub module: String,
    pub file: String,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reexport {
    pub name: String,
    pub trace: Vec<Hop>,
    pub end: End,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hop {
    pub module: String,
    pub file: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum End {
    Definition,
    External,
    Unresolved,
}

/// Declares structs of summable fields.
macro_rules! summable {
    ($($name:ident: $ty:ty { $($field:ident),* $(,)? })*) => {$(
        #[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            $(pub $field: $ty,)*
        }

        impl AddAssign for $name {
            fn add_assign(&mut self, other: Self) {
                $(self.$field += other.$field;)*
            }
        }
    )*};
}

summable! {
    Lines: u32 { code, test, comment, blank }
    Tests: u32 { count, ignored }
    Items: u32 {
        r#struct, r#enum, union, r#trait, r#impl, r#fn, method,
        r#const, r#static, r#type, r#mod, r#macro,
    }
    Functional: u32 { closures, combinators, higher_order_fns }
    Suppressions: u32 { todo, unimplemented, unreachable, allow }
    ErrorHandling: u32 { unwrap, expect, panic, propagate }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub lines: Lines,
    pub tests: Tests,
    pub items: Items,
    pub functional: Functional,
    pub suppressions: Suppressions,
    pub error_handling: ErrorHandling,
}

impl AddAssign for Metrics {
    fn add_assign(&mut self, other: Self) {
        self.lines += other.lines;
        self.tests += other.tests;
        self.items += other.items;
        self.functional += other.functional;
        self.suppressions += other.suppressions;
        self.error_handling += other.error_handling;
    }
}

impl AddAssign for Totals {
    fn add_assign(&mut self, other: Self) {
        self.files += other.files;
        self.metrics += other.metrics;
    }
}
