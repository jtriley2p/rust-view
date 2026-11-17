# rust-view-core

Static metrics for a cargo workspace, exported as JSON for an external visualization tool.
The output format is specified by `spec/report.schema.json`; `spec/example.json` shows it.

## Decided

### Boundary

- Core is pure: no filesystem, no process spawning, no manifest parsing.
- `analyze(&[CrateInput]) -> Analysis { report, errors }`.
- The CLI supplies source text, target roots, declared dependencies, and the resolved package
  graph (from `cargo metadata`). Core passes the last two through untouched.
- `analyze` never fails. Bad input is skipped and listed in `errors`
  (`Parse`, `MissingModule`, `Orphan`); errors are not part of the JSON.

### Report

- Shape: `report -> crates[] -> files[]`, with `totals` on the report and on each crate.
- Per file: `root`, `lines`, `tests`, `items`, `functional`, `suppressions`, `error_handling`,
  `definitions`, `imports`, `reexports`.
- `root` names the file's target. Module paths repeat across a crate's targets (each has its own
  `crate`), so consumers key modules by root and module.
- Per crate: `version`, `dependencies` (declared: name, req, kind, source), and `packages`
  (resolved: the crate plus everything it transitively depends on, each with name, version,
  source, and edges by name, version, and kind).
- Each report carries its crate's whole closure, so one report draws a full graph; consumers
  merge packages across reports by name and version.
- Counts, plus names where a consumer needs them to connect things (`definitions`, `imports`,
  `reexports`). No line locations.
- Totals sum the per-file counts, plus a `files` count. Definitions, imports, and re-exports are
  not totalled.
- Unknown keys are rejected; any shape change bumps `version`.
- Dropped: test coverage, dead item count.

### Rules

- **Test code** is `#[cfg(test)]` items, test fns, files under `tests/`, and files reached
  through a test-only `mod`. Everything except `lines.test` and `tests` excludes it.
- **Test fn**: any attribute whose last path segment is `test` (covers `#[tokio::test]`).
- **`cfg`**: `cfg(all(test, ..))` is test code, `cfg(not(test))` is not. Other `cfg` gates are
  ignored; all code is counted.
- **Lines**: code if a token touches the line, else blank if empty, else doc if a doc comment
  touches it, else comment. `#[doc = ..]` attributes are doc comments; `#[doc(..)]` is code.
  Doc lines inside test code stay doc.
- **Definitions**: each named item with its kind, defining module (inline modules included), and
  whether it has a visibility modifier (`pub(crate)` counts; a macro needs `#[macro_export]`).
  Left out: `mod`s (the module paths carry them), `impl` blocks, methods, `const _`, and items
  local to a fn body.
- **Imports**: one edge per target module, keyed by file. `use` declarations plus qualified
  `crate`/`self`/`super` paths. Same-file targets are omitted. Re-exports are not followed.
- **Unprefixed `use` paths** resolve when the current module defines the first segment.
- **Re-exports**: each `pub use` name is traced hop by hop to its definition, ending in
  `definition`, `external`, or `unresolved`. `pub(crate) use` counts.
- **Macro bodies**: a call in item position is parsed as items. Failing that, as an expression
  list; failing that, its brace groups that hold items are visited and each name after an item
  keyword is recorded as a definition. Any other call is parsed as an expression list or skipped.
- **Combinators** are matched by method name.

## Worth knowing

- **Macro-generated names are invisible.** Items written inside a call are seen; names a macro
  builds from its arguments or a derive generates are not, and re-exports of them end as
  `unresolved` (81 of 123 in http).
- **Scraped names are fuzzy.** They serve name resolution only, are neither counted as items nor
  listed in `definitions`, and can be wrong
  (a method in an `impl`-like macro, `N` in `<const N: usize>`). A scraped `mod` is not followed.
- **Foreign items are not definitions.** `fn`s and `static`s in `extern` blocks are not
  recorded; re-exports of them end as `unresolved` (most of libc's 222).
- **Combinator matching is fuzzy.** `take` and `last` are left out to avoid `Option::take` and
  slice `last`; false positives such as `str::find` remain.
- **Orphans are common.** Fixture files (e.g. trybuild's `tests/ui/*.rs`) are reported as
  `Orphan`; the CLI should treat them as warnings.
- **`public` is the modifier, not reachability.** A `pub` item in a private module is `public`.
- **Re-exports in inline modules lose their scope.** They are listed under the file with no
  record of the inline module they are exported from.
- **A binary importing its own library** (`use my_pkg::x` in `main.rs`) is treated as external.
- **A file shared by two targets** is attributed to the first root that reaches it.
- **`packages` is as cargo resolves it**: every platform's dependencies, default features of
  the workspace, and dev-dependencies of workspace members only (so a path dependency brings its
  dev-dependencies along). It is empty when resolution fails, e.g. offline with a cold cache.
- **Packages are keyed by name and version.** The same name and version from two sources
  (a registry crate patched by a git fork) collapse into one.
- **syn is 3.x.**
- **`jsonschema` is a heavy dev-dependency**; the library itself depends only on `syn`,
  `proc-macro2`, and `serde`.

## Remaining

- **Spec**: add a `module` field to re-exports to keep inline-module scope.
- **Resolution**: resolve a binary's imports of its own library; record `extern` block items as
  definitions.
- **Macro expansion**, if revisited: have the CLI run `cargo rustc -- -Zunpretty=expanded`
  (nightly, needs a successful build, one cfg set) and pass the text into core as data, scanned
  for definitions only. A `macro_rules!` interpreter in core would stay pure but miss dependency
  macros and proc macros.
