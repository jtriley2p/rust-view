//! Per-file facts gathered in one pass over the syntax tree.

use crate::lines;
use crate::report::{ItemKind, Metrics};
use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, Item, Meta, Token, UseTree};

/// Method names counted as functional combinators.
const COMBINATORS: &[&str] = &[
    "all",
    "and_then",
    "any",
    "chain",
    "cloned",
    "collect",
    "copied",
    "count",
    "cycle",
    "enumerate",
    "filter",
    "filter_map",
    "find",
    "find_map",
    "flat_map",
    "flatten",
    "fold",
    "for_each",
    "fuse",
    "inspect",
    "is_none_or",
    "is_ok_and",
    "is_some_and",
    "map",
    "map_err",
    "map_or",
    "map_or_else",
    "map_while",
    "max_by",
    "max_by_key",
    "min_by",
    "min_by_key",
    "ok_or",
    "ok_or_else",
    "or_else",
    "partition",
    "peekable",
    "position",
    "product",
    "reduce",
    "rev",
    "scan",
    "skip",
    "skip_while",
    "step_by",
    "sum",
    "take_while",
    "then",
    "then_some",
    "try_fold",
    "try_for_each",
    "unwrap_or_else",
    "unzip",
    "zip",
];

#[derive(Debug, Default)]
pub struct Scan {
    /// Everything but test code, except `lines.test` and `tests`.
    pub metrics: Metrics,
    pub mods: Vec<Mod>,
    pub uses: Vec<Use>,
    pub defs: Vec<Def>,
}

/// A named item. `scope` is the chain of inline modules enclosing it.
#[derive(Debug)]
pub struct Def {
    pub scope: Vec<String>,
    pub name: String,
    /// None for a `mod`, an unnamed `const`, an item local to a block, and a name scraped
    /// from a macro body.
    pub kind: Option<ItemKind>,
    /// Has a visibility modifier; `#[macro_export]` for a macro.
    pub public: bool,
}

/// Keywords that introduce a named item.
const ITEM_KEYWORDS: &[&str] = &[
    "struct", "enum", "union", "trait", "fn", "type", "const", "static", "mod",
];

/// A `mod` item. `scope` as in [`Def`].
#[derive(Debug)]
pub struct Mod {
    pub scope: Vec<String>,
    pub name: String,
    /// `#[path = ".."]`
    pub path: Option<String>,
    pub inline: bool,
    pub test: bool,
}

/// One leaf of a `use` tree, or a qualified `crate`/`self`/`super` path.
#[derive(Debug)]
pub struct Use {
    pub scope: Vec<String>,
    /// Path segments; a glob's stop before the `*`.
    pub path: Vec<String>,
    /// Name brought into scope; `*` for a glob.
    pub name: String,
    /// Has a visibility modifier, so it re-exports.
    pub public: bool,
}

pub fn scan(text: &str) -> syn::Result<Scan> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // A shebang is not Rust; lex from its newline so line numbers hold.
    let body = match text.strip_prefix("#!") {
        Some(rest) if !rest.trim_start().starts_with('[') => {
            &text[text.find('\n').unwrap_or(text.len())..]
        }
        _ => text,
    };
    let tokens: TokenStream = body.parse()?;
    let file: syn::File = syn::parse2(tokens.clone())?;

    let mut v = Visitor::default();
    if file.attrs.iter().any(is_cfg_test) {
        v.test_region((1, usize::MAX), |v| v.visit_file(&file));
    } else {
        v.visit_file(&file);
    }
    v.out.metrics.lines = lines::count(text, tokens, &v.tests);
    Ok(v.out)
}

#[derive(Default)]
struct Visitor {
    out: Scan,
    /// Inline modules enclosing the current node.
    scope: Vec<String>,
    /// Blocks enclosing the current node.
    blocks: u32,
    /// Line ranges of test code.
    tests: Vec<(usize, usize)>,
}

impl Visitor {
    /// Runs `f` over test code: only its test counts and `mod` declarations are kept.
    fn test_region(&mut self, lines: (usize, usize), f: impl FnOnce(&mut Self)) {
        let outer = std::mem::take(&mut self.out.metrics);
        let (uses, defs, mods) = (
            self.out.uses.len(),
            self.out.defs.len(),
            self.out.mods.len(),
        );
        f(self);
        let inner = std::mem::replace(&mut self.out.metrics, outer);
        self.out.metrics.tests += inner.tests;
        self.out.uses.truncate(uses);
        self.out.defs.truncate(defs);
        self.out.mods[mods..].iter_mut().for_each(|m| m.test = true);
        self.tests.push(lines);
    }

    fn item(&mut self, item: &Item) {
        let m = &mut self.out.metrics;
        let vis = |v: &syn::Visibility| !matches!(v, syn::Visibility::Inherited);
        let (count, name, kind, public) = match item {
            Item::Struct(i) => (
                &mut m.items.r#struct,
                &i.ident,
                ItemKind::Struct,
                vis(&i.vis),
            ),
            Item::Enum(i) => (&mut m.items.r#enum, &i.ident, ItemKind::Enum, vis(&i.vis)),
            Item::Union(i) => (&mut m.items.union, &i.ident, ItemKind::Union, vis(&i.vis)),
            Item::Trait(i) => (&mut m.items.r#trait, &i.ident, ItemKind::Trait, vis(&i.vis)),
            Item::Fn(i) => (&mut m.items.r#fn, &i.sig.ident, ItemKind::Fn, vis(&i.vis)),
            Item::Const(i) => (&mut m.items.r#const, &i.ident, ItemKind::Const, vis(&i.vis)),
            Item::Static(i) => (
                &mut m.items.r#static,
                &i.ident,
                ItemKind::Static,
                vis(&i.vis),
            ),
            Item::Type(i) => (&mut m.items.r#type, &i.ident, ItemKind::Type, vis(&i.vis)),
            Item::Macro(syn::ItemMacro {
                ident: Some(ident),
                attrs,
                ..
            }) => {
                let exported = attrs.iter().any(|a| a.path().is_ident("macro_export"));
                (&mut m.items.r#macro, ident, ItemKind::Macro, exported)
            }
            Item::Mod(i) => {
                m.items.r#mod += 1;
                self.def(i.ident.to_string(), None, vis(&i.vis));
                return visit::visit_item(self, item);
            }
            Item::Impl(_) => {
                m.items.r#impl += 1;
                return visit::visit_item(self, item);
            }
            _ => return visit::visit_item(self, item),
        };
        *count += 1;
        if let Item::Fn(f) = item
            && f.attrs.iter().any(is_test)
        {
            m.tests.count += 1;
            m.tests.ignored += f.attrs.iter().any(|a| a.path().is_ident("ignore")) as u32;
        }
        // `const _` names nothing.
        self.def(name.to_string(), (name != "_").then_some(kind), public);
        visit::visit_item(self, item)
    }

    fn def(&mut self, name: String, kind: Option<ItemKind>, public: bool) {
        self.out.defs.push(Def {
            scope: self.scope.clone(),
            name,
            kind: kind.filter(|_| self.blocks == 0),
            public,
        });
    }

    /// Counts a macro call and visits its arguments; false if they are not an expression list.
    fn call(&mut self, i: &syn::Macro) -> bool {
        let m = &mut self.out.metrics;
        match i
            .path
            .segments
            .last()
            .map(|s| s.ident.to_string())
            .as_deref()
        {
            Some("todo") => m.suppressions.todo += 1,
            Some("unimplemented") => m.suppressions.unimplemented += 1,
            Some("unreachable") => m.suppressions.unreachable += 1,
            Some("panic") => m.error_handling.panic += 1,
            _ => {}
        }
        // Macro bodies are opaque to syn; most call-like macros take expressions.
        let args = i.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated);
        args.iter().flatten().for_each(|e| self.visit_expr(e));
        visit::visit_macro(self, i);
        args.is_ok()
    }

    /// Fallback for an item-position macro body with syntax of its own: visits brace groups
    /// that hold items, and records the name after an item keyword as a definition.
    fn scrape(&mut self, tokens: TokenStream) {
        let mut keyword = false;
        for token in tokens {
            match token {
                TokenTree::Ident(i) => match i.to_string().as_str() {
                    k if ITEM_KEYWORDS.contains(&k) => keyword = true,
                    // `static mut X`, `static ref X`, `const unsafe fn x`, `const _`
                    "mut" | "ref" | "unsafe" | "async" | "extern" | "_" => {}
                    name if keyword => {
                        self.def(name.to_string(), None, false);
                        keyword = false;
                    }
                    _ => {}
                },
                TokenTree::Group(g) => {
                    keyword = false;
                    if g.delimiter() == Delimiter::Brace
                        && let Ok(file) = syn::parse2::<syn::File>(g.stream())
                    {
                        self.visit_file(&file);
                    }
                }
                _ => keyword = false,
            }
        }
    }

    fn use_tree(&mut self, tree: &UseTree, prefix: &mut Vec<String>, public: bool) {
        let (ident, rename) = match tree {
            UseTree::Path(p) => {
                prefix.push(p.ident.to_string());
                self.use_tree(&p.tree, prefix, public);
                prefix.pop();
                return;
            }
            UseTree::Group(g) => {
                return g
                    .items
                    .iter()
                    .for_each(|t| self.use_tree(t, prefix, public));
            }
            UseTree::Glob(_) => (None, Some("*".to_string())),
            UseTree::Name(n) => (Some(n.ident.to_string()), None),
            UseTree::Rename(r) => (Some(r.ident.to_string()), Some(r.rename.to_string())),
        };
        let mut path = prefix.clone();
        // `a::{self}` names `a` itself.
        path.extend(ident.filter(|i| i != "self"));
        let name = rename.or_else(|| path.last().cloned()).unwrap_or_default();
        self.out.uses.push(Use {
            scope: self.scope.clone(),
            path,
            name,
            public,
        });
    }
}

impl<'ast> Visit<'ast> for Visitor {
    fn visit_item(&mut self, i: &'ast Item) {
        match is_test_code(attrs(i)) {
            true => self.test_region(line_range(i), |v| v.item(i)),
            false => self.item(i),
        }
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        let path = i.attrs.iter().find_map(|a| match &a.meta {
            Meta::NameValue(nv) if nv.path.is_ident("path") => match &nv.value {
                Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) => Some(s.value()),
                _ => None,
            },
            _ => None,
        });
        self.out.mods.push(Mod {
            scope: self.scope.clone(),
            name: i.ident.to_string(),
            path,
            inline: i.content.is_some(),
            test: false,
        });
        self.scope.push(i.ident.to_string());
        visit::visit_item_mod(self, i);
        self.scope.pop();
    }

    fn visit_block(&mut self, i: &'ast syn::Block) {
        self.blocks += 1;
        visit::visit_block(self, i);
        self.blocks -= 1;
    }

    fn visit_item_use(&mut self, i: &'ast syn::ItemUse) {
        let public = !matches!(i.vis, syn::Visibility::Inherited);
        self.use_tree(&i.tree, &mut Vec::new(), public);
        visit::visit_item_use(self, i);
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_test_code(&i.attrs) {
            return self.test_region(line_range(i), |v| visit::visit_impl_item_fn(v, i));
        }
        self.out.metrics.items.method += 1;
        visit::visit_impl_item_fn(self, i);
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        self.out.metrics.items.method += 1;
        visit::visit_trait_item_fn(self, i);
    }

    fn visit_signature(&mut self, i: &'ast syn::Signature) {
        let mut takes_fn = TakesFn(false);
        visit::visit_signature(&mut takes_fn, i);
        self.out.metrics.functional.higher_order_fns += takes_fn.0 as u32;
        visit::visit_signature(self, i);
    }

    fn visit_expr_closure(&mut self, i: &'ast syn::ExprClosure) {
        self.out.metrics.functional.closures += 1;
        visit::visit_expr_closure(self, i);
    }

    fn visit_expr_method_call(&mut self, i: &'ast syn::ExprMethodCall) {
        let m = &mut self.out.metrics;
        match i.method.to_string().as_str() {
            "unwrap" => m.error_handling.unwrap += 1,
            "expect" => m.error_handling.expect += 1,
            name if COMBINATORS.contains(&name) => m.functional.combinators += 1,
            _ => {}
        }
        visit::visit_expr_method_call(self, i);
    }

    fn visit_expr_try(&mut self, i: &'ast syn::ExprTry) {
        self.out.metrics.error_handling.propagate += 1;
        visit::visit_expr_try(self, i);
    }

    fn visit_item_macro(&mut self, i: &'ast syn::ItemMacro) {
        i.attrs.iter().for_each(|a| self.visit_attribute(a));
        if i.ident.is_some() {
            // `macro_rules!` holds rules, not items.
            self.call(&i.mac);
        } else if let Ok(file) = syn::parse2::<syn::File>(i.mac.tokens.clone()) {
            // A call in item position usually wraps the items it defines.
            self.visit_path(&i.mac.path);
            self.visit_file(&file);
        } else if !self.call(&i.mac) {
            self.scrape(i.mac.tokens.clone());
        }
    }

    fn visit_macro(&mut self, i: &'ast syn::Macro) {
        self.call(i);
    }

    fn visit_attribute(&mut self, i: &'ast Attribute) {
        let allow = i.path().is_ident("allow") || i.path().is_ident("expect");
        self.out.metrics.suppressions.allow += allow as u32;
    }

    fn visit_path(&mut self, i: &'ast syn::Path) {
        let first = i
            .segments
            .first()
            .map(|s| s.ident.to_string())
            .unwrap_or_default();
        if i.segments.len() > 1 && matches!(first.as_str(), "crate" | "self" | "super") {
            let path: Vec<String> = i.segments.iter().map(|s| s.ident.to_string()).collect();
            let name = path.last().cloned().unwrap_or_default();
            self.out.uses.push(Use {
                scope: self.scope.clone(),
                path,
                name,
                public: false,
            });
        }
        visit::visit_path(self, i);
    }

    /// `pub(in path)` is not an import.
    fn visit_vis_restricted(&mut self, _: &'ast syn::VisRestricted) {}
}

/// Finds a function type in a signature: fn pointer, or `Fn*` bound.
struct TakesFn(bool);

impl<'ast> Visit<'ast> for TakesFn {
    fn visit_type_fn_ptr(&mut self, _: &'ast syn::TypeFnPtr) {
        self.0 = true;
    }

    fn visit_trait_bound(&mut self, i: &'ast syn::TraitBound) {
        let name = i.path.segments.last().map(|s| s.ident.to_string());
        self.0 |= matches!(
            name.as_deref(),
            Some("Fn" | "FnMut" | "FnOnce" | "AsyncFn" | "AsyncFnMut" | "AsyncFnOnce")
        );
    }
}

fn attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(i) => &i.attrs,
        Item::Enum(i) => &i.attrs,
        Item::ExternCrate(i) => &i.attrs,
        Item::Fn(i) => &i.attrs,
        Item::ForeignMod(i) => &i.attrs,
        Item::Impl(i) => &i.attrs,
        Item::Macro(i) => &i.attrs,
        Item::Mod(i) => &i.attrs,
        Item::Static(i) => &i.attrs,
        Item::Struct(i) => &i.attrs,
        Item::Trait(i) => &i.attrs,
        Item::TraitAlias(i) => &i.attrs,
        Item::Type(i) => &i.attrs,
        Item::Union(i) => &i.attrs,
        Item::Use(i) => &i.attrs,
        _ => &[],
    }
}

fn is_test_code(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| is_test(a) || is_cfg_test(a))
}

/// `#[test]`, `#[tokio::test]`, ..
fn is_test(attr: &Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|s| s.ident == "test")
}

/// `#[cfg(test)]`, or any `cfg` naming `test` outside a `not(..)`.
fn is_cfg_test(attr: &Attribute) -> bool {
    fn names_test(tokens: TokenStream) -> bool {
        let mut not = false;
        tokens.into_iter().any(|t| match t {
            TokenTree::Ident(i) => {
                not = i == "not";
                i == "test"
            }
            TokenTree::Group(g) => !std::mem::take(&mut not) && names_test(g.stream()),
            _ => false,
        })
    }
    matches!(&attr.meta, Meta::List(l) if l.path.is_ident("cfg") && names_test(l.tokens.clone()))
}

fn line_range(node: &impl Spanned) -> (usize, usize) {
    let span = node.span();
    (span.start().line, span.end().line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::*;

    #[test]
    fn counts() {
        let s = scan(
            r#"
            #![allow(dead_code)]
            pub struct A;
            enum B { X }
            union U { a: u8 }
            trait T { fn m(&self); }
            impl T for A { fn m(&self) {} }
            const C: u8 = 0;
            static S: u8 = 0;
            type Alias = A;
            mod inner { pub fn g() {} }
            macro_rules! mac { () => { x.unwrap() } }

            fn apply(f: impl Fn(u8) -> u8, g: fn(u8), x: u8) -> u8 { f(x) }
            fn make<F: FnOnce()>(f: F) where F: Send {}

            #[expect(unused)]
            fn run(v: Vec<u8>) -> Result<u8, ()> {
                let n = v.iter().map(|x| x + 1).filter(|x| *x > 1).count();
                let a = v.first().unwrap();
                let b = v.last().expect("last");
                println!("{}", v.get(0).unwrap());
                let c = parse()?;
                if n > 9 { panic!("no") }
                match a { 0 => todo!(), 1 => unimplemented!(), _ => unreachable!() }
            }
            "#,
        )
        .unwrap();
        let m = s.metrics;
        assert_eq!(
            m.items,
            Items {
                r#struct: 1,
                r#enum: 1,
                union: 1,
                r#trait: 1,
                r#impl: 1,
                r#fn: 4,
                method: 2,
                r#const: 1,
                r#static: 1,
                r#type: 1,
                r#mod: 1,
                r#macro: 1,
            }
        );
        assert_eq!(
            m.functional,
            Functional {
                closures: 2,
                combinators: 3,
                higher_order_fns: 2
            }
        );
        assert_eq!(
            m.suppressions,
            Suppressions {
                todo: 1,
                unimplemented: 1,
                unreachable: 1,
                allow: 2
            }
        );
        assert_eq!(
            m.error_handling,
            ErrorHandling {
                unwrap: 2,
                expect: 1,
                panic: 1,
                propagate: 1
            }
        );
        assert_eq!(m.tests, Tests::default());
    }

    #[test]
    fn defs() {
        let s = scan(
            r#"
            pub struct A;
            pub(crate) enum B {}
            trait T { fn m(&self); }
            impl A { pub fn m(&self) {} }
            fn f() { struct Local; }
            const _: () = ();
            #[macro_export]
            macro_rules! mac { () => {} }
            macro_rules! own { () => {} }
            pub mod m { pub(super) static S: u8 = 0; type Alias = u8; }
            "#,
        )
        .unwrap();
        let got: Vec<_> = s
            .defs
            .iter()
            .filter_map(|d| Some((d.scope.join("::"), d.name.as_str(), d.kind?, d.public)))
            .collect();
        assert_eq!(
            got,
            [
                ("".into(), "A", ItemKind::Struct, true),
                ("".into(), "B", ItemKind::Enum, true),
                ("".into(), "T", ItemKind::Trait, false),
                ("".into(), "f", ItemKind::Fn, false),
                ("".into(), "mac", ItemKind::Macro, true),
                ("".into(), "own", ItemKind::Macro, false),
                ("m".into(), "S", ItemKind::Static, true),
                ("m".into(), "Alias", ItemKind::Type, false),
            ]
        );
        let untyped: Vec<_> = s.defs.iter().filter(|d| d.kind.is_none()).collect();
        let names: Vec<_> = untyped.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["Local", "_", "m"]);
    }

    #[test]
    fn test_code_is_excluded() {
        let text = "\
fn f() {}
#[cfg(test)]
mod tests {
    use super::*;
    mod helpers;
    #[test]
    fn a() { f().unwrap(); }
    #[tokio::test]
    #[ignore]
    async fn b() { todo!() }
}
#[cfg(not(test))]
fn g() {}
#[test]
fn c() {}
";
        let s = scan(text).unwrap();
        let m = s.metrics;
        assert_eq!(
            m.tests,
            Tests {
                count: 3,
                ignored: 1
            }
        );
        assert_eq!(
            m.items,
            Items {
                r#fn: 2,
                ..Default::default()
            }
        );
        assert_eq!(m.error_handling, ErrorHandling::default());
        assert_eq!(m.suppressions, Suppressions::default());
        assert_eq!(
            m.lines,
            Lines {
                code: 3,
                test: 12,
                doc: 0,
                comment: 0,
                blank: 0
            }
        );
        assert!(s.uses.is_empty());
        let defs: Vec<_> = s.defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(defs, ["f", "g"]);
        let helpers = s.mods.iter().find(|m| m.name == "helpers").unwrap();
        assert!(helpers.test && !helpers.inline && helpers.scope == ["tests"]);
    }

    #[test]
    fn whole_file_cfg_test() {
        let s = scan("#![cfg(test)]\nfn f() {}\n#[test]\nfn t() {}\n").unwrap();
        assert_eq!(
            s.metrics.lines,
            Lines {
                code: 0,
                test: 4,
                doc: 0,
                comment: 0,
                blank: 0
            }
        );
        assert_eq!(s.metrics.tests.count, 1);
        assert_eq!(s.metrics.items, Items::default());
    }

    #[test]
    fn uses() {
        let s = scan(
            r#"
            use crate::a::{self, B, c::D as E, f::*};
            pub use self::g::H;
            pub(crate) use super::I as _;
            mod m {
                #[path = "x.rs"]
                mod n;
                fn f() -> crate::a::B { super::k::<u8>() }
            }
            pub(in crate::a) fn vis() {}
            "#,
        )
        .unwrap();
        let got: Vec<_> = s
            .uses
            .iter()
            .map(|u| {
                (
                    u.scope.join("::"),
                    u.path.join("::"),
                    u.name.as_str(),
                    u.public,
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                ("".into(), "crate::a".into(), "a", false),
                ("".into(), "crate::a::B".into(), "B", false),
                ("".into(), "crate::a::c::D".into(), "E", false),
                ("".into(), "crate::a::f".into(), "*", false),
                ("".into(), "self::g::H".into(), "H", true),
                ("".into(), "super::I".into(), "_", true),
                ("m".into(), "crate::a::B".into(), "B", false),
                ("m".into(), "super::k".into(), "k", false),
            ]
        );
        let n = s.mods.iter().find(|m| m.name == "n").unwrap();
        assert_eq!(
            (n.scope.join("::"), n.path.as_deref(), n.inline),
            ("m".into(), Some("x.rs"), false)
        );
        assert!(s.mods.iter().any(|m| m.name == "m" && m.inline));
    }

    #[test]
    fn item_macros() {
        let s = scan(
            r#"
            wrap! {
                #![allow(unused)]
                pub struct A;
                mod file;
                pub use self::file::B;
                nested! { fn f() { x.unwrap(); } }
                #[cfg(test)]
                mod tests;
            }
            cfg_if! {
                if #[cfg(unix)] { mod unix; pub enum C {} } else { mod other; }
            }
            keywords! { "abstract" pub struct D "as" pub struct E }
            flags! { pub struct F: u8 { const X = 1; } }
            custom! { lazy static ref G: u8 = 0; const unsafe fn h() {} const _: () = (); }
            call!(a.unwrap(), b);
            macro_rules! rules { () => { struct NotAnItem; } }
            mod m { wrap! { struct In; } }
            fn body() { local! { struct Local; } quote! { fn quoted() {} }; }
            "#,
        )
        .unwrap();
        let defs: Vec<_> = s
            .defs
            .iter()
            .map(|d| {
                d.scope
                    .iter()
                    .chain([&d.name])
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .collect();
        assert_eq!(
            defs,
            [
                "A", "file", "f", "unix", "C", "other", "D", "E", "F", "G", "h", "rules", "m",
                "m::In", "body"
            ]
        );
        let mods: Vec<_> = s.mods.iter().map(|m| (m.name.as_str(), m.test)).collect();
        assert_eq!(
            mods,
            [
                ("file", false),
                ("tests", true),
                ("unix", false),
                ("other", false),
                ("m", false)
            ]
        );
        let uses: Vec<_> = s
            .uses
            .iter()
            .map(|u| (u.path.join("::"), u.public))
            .collect();
        assert_eq!(uses, [("self::file::B".to_string(), true)]);
        // Scraped names are definitions only; they are not counted as items.
        assert_eq!(
            s.metrics.items,
            Items {
                r#struct: 2,
                r#enum: 1,
                r#fn: 2,
                r#mod: 4,
                r#macro: 1,
                ..Default::default()
            }
        );
        assert_eq!(s.metrics.error_handling.unwrap, 2);
        assert_eq!(s.metrics.suppressions.allow, 1);
        assert_eq!(s.metrics.lines.test, 2);
    }

    #[test]
    fn shebang_and_errors() {
        let s = scan("#!/usr/bin/env cargo\nfn main() {}\n").unwrap();
        assert_eq!(
            s.metrics.lines,
            Lines {
                code: 1,
                test: 0,
                doc: 0,
                comment: 1,
                blank: 0
            }
        );
        let e = scan("fn ok() {}\nfn 1() {}\n").unwrap_err();
        assert_eq!(e.span().start().line, 2);
        assert!(scan("let s = \"unterminated;\n").is_err());
    }
}
