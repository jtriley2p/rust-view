# Value flow

Plan for a "Value flow" dashboard screen: for a program traced from a chosen fn, a matrix of
what each call does to each tracked value. Not started. Written 2026-11-18.

## Decided

- **Analysis uses rust-analyzer's library crates** (`ra_ap_*`), not syn. Core reads signatures
  only; doing this by hand would mean rebuilding type inference and method resolution.
- **A new crate, `crates/values`**, holds it. It loads a workspace from disk, so it cannot live in
  core. Report types stay in core's `report.rs`. It is exempt from "minimize deps".
- **The report holds one body per fn; the web stitches them** into a trace for whichever root is
  picked. The report stays linear in code size and any fn can be the root.
- **Existing analysis is untouched**: `scan.rs`, `resolve.rs`, and `flow.rs` stay on syn. Moving
  them onto rust-analyzer is a separate decision.

Assumed, not yet confirmed:

- The CLI always runs the value analysis; a flag to skip it comes only if the cost bites.
- Proc macros are not expanded (that needs a build of the target).
- Bodies carry line locations (`src`), reversing core.md's "No line locations" for this data.
- A value's column group is the fn that binds it.

## Spike findings

A throwaway program loaded this workspace with `ra_ap_*` 0.0.358 and walked every fn body.

- 78 fns, 1,031 calls: 6 unresolved (proc-macro2's `Span::start`/`end`, one `.max`), 13 bindings
  untyped (patterns such as `None`, which are constants; check `resolve_bind_pat_to_const`).
- Resolves std methods, closures, calls inside expanded macros, and calls into a workspace
  sibling (`rust_view_core::analyze` from the CLI), so a trace can cross crates.
- Every local has a type, `is_copy`, and `drop_glue`. Receivers report their auto-borrow
  (`expr_adjustments`); closures report each capture and its kind (`captured_items`).
- No borrow checker is exposed in this version: moves are classified here, not read off.
- Cost: 81 -> 179 locked packages, ~40 s cold release build, ~3.5 s per run on this workspace
  (mostly one-time std setup), ~1 GB peak memory.

Needed to make it build and run:

- Pin every `ra_ap_*` crate to `=0.0.358`. Newer versions are published piecemeal (`ra_ap_hir`
  0.0.363 while `ra_ap_load-cargo` is 0.0.358); there is no semver, so an upgrade is deliberate.
- Hold `unicode-properties` at 0.1.4 in `Cargo.lock`: `ra-ap-rustc_lexer` requires it to share a
  Unicode version with `unicode-ident`, and 0.1.5 does not.
- `anyhow` is a dependency: `load_workspace_at` returns its `Result`.
- Run every type query inside `hir::attach_db(&db, || ..)`; outside it rust-analyzer panics.
- Load with `sysroot: Some(RustLibSource::Discover)`, `ProcMacroServerChoice::None`, and
  `load_out_dirs_from_check: false`.
- A node inside a macro expansion has positions relative to the expansion; use the call's line.

## Contract

Schema version 6 -> 7. Each file gains `bodies`, beside `flows`; empty when not analyzed.

```
body: { src, name, module, params, values: [{ name, type }], steps: [step] }
step: { depth, name, src, frame?, note?, events: { <value>: C|R|M|V|D }, call?, args? }
```

- `src` is `path:line`, workspace-relative. A body's `src` is its key: unique where names are not.
- `values` are the fn's bindings in order; the first `params` of them are its parameters. `type`
  is as rust-analyzer displays it.
- `steps` are in evaluation order (receiver and arguments before the call); `depth` nests a
  frame's or a closure's calls under its row. `events` are keyed by index into `values`.
- `call` is the callee's `src` when it is a fn of the workspace. `args` gives, per callee
  parameter, the caller's value passed for it, or null for a temporary.
- `frame` is `loop`, `match`, `async`, `spawn`, or `end`. `note` is free text.
- A shape change touches the schema, `report.rs`, `report.ts`, and `example.json`.

## Rules

Events (C constructed, R read, M mutated, V moved, D dropped):

- **C**: a binding, on the row of the call that produced it.
- **R**: `&x`; a receiver auto-borrowed shared; a bare use of a `&T` or `Copy` local; a shared
  capture.
- **M**: `&mut x`; a receiver auto-borrowed mutably; a bare use of a `&mut T` local; an
  assignment to it or its fields; a mutable capture.
- **V**: a bare use of a non-`Copy` local (argument, `let y = x`, struct field, return, by-value
  receiver); a `move` capture.
- **D**: `drop(x)`; scope end for a local with drop glue that was never moved.

Rows:

- A call is a step if it resolves into the workspace or carries an event; other calls are noise.
- An event no call carries (a literal or struct-literal binding, an assignment) gets a row named
  for the expression.
- N identical consecutive steps merge into one, with note `×N`.
- `loop` is a `for`/`while`/`loop` body and `match` a match, each noted with its header; `async`
  is an async block; `spawn` is a closure passed to a callee named `spawn*`; `end` carries
  scope-end drops and is left out when there are none. `if`/`else` gives no frame.
- Calls inside a closure passed as an argument are children of that call's row.

## Steps

1. **Contract**: schema, `report.rs`, `report.ts` (types and shape check), `example.json` with a
   hand-written body, core.md.
2. **`crates/values`**: `bodies(manifest) -> (bodies by file, errors)`. Errors are enumerated; a
   panic inside rust-analyzer is caught per fn and reported, not propagated.
3. **CLI**: load once per `export`, merge bodies into each report by file path. On failure, warn
   and write empty `bodies`, as for the dependency graph.
4. **Web**: `web/src/values.tsx`, added to `SCREENS` in `main.tsx`; see View.
5. **Tests**: a fixture cargo project for `crates/values` (rust-analyzer needs one on disk), the
   spec round-trip, `bun run check`.
6. **Docs**: core.md (decisions, limits), CLAUDE.md (the new crate).

2-3 and 4 are independent once 1 lands: the web can be built against the example.

## View

Stitching, in the web: `trace(crates, root)` inlines each step's `call` body under it, mapping
the callee's parameters to the caller's values through `args` and giving its other bindings
fresh columns. A call to a body already on the stack is a leaf noted `recursive`; depth and row
counts are capped. The default root is a binary's `main`, else the body with the most steps.

That yields the view's data:

- `rows`: calls in preorder. `{ depth, name, frame?, src, note?, events: { valueKey: kind } }`
- `values`: `{ key, name, type, group }`, ordered by first use; groups are contiguous.
- Derived once: each row's `parent` and `end` (index of its last descendant); each value's
  `first` and `last` event row.

Page: a toolbar (root picker, value filter, Expand all, Collapse all, stats, legend); below it
the matrix, taking the remaining width, and an inspector panel.

Matrix:

- One horizontally scrollable box.
- Left column: the call tree. Indented, expandable if the row has children, optional frame
  label, name, then meta (the note, or "N calls" when collapsed).
- One column per value. Above them a group label row, then vertical headers reading "name Type".
- Drawn as an HTML grid for the sticky tree column and headers, with one SVG for the body: a
  line per value and a mark per event, not an element per cell.

Cells:

- Visible rows are those with no collapsed ancestor.
- A row's span is itself if expanded or a leaf; itself through `end` if collapsed.
- A cell's mark is the strongest event for that value within the span: C > V > D > M > R.
- A lifeline is a vertical line where the value's `[first, last]` overlaps the span. The top half
  is drawn only if `first` is before the row; the bottom half only if `last` is after the span.
- Marks: constructed a filled square, read a hollow square, mutated a half-filled square, moved
  a right-pointing triangle, dropped a cross.
- A value keeps its own column after being moved into another value.

Interaction:

- Clicking a tree row toggles its subtree. Expand all clears every collapse; Collapse all
  collapses everything below the root.
- Clicking a mark selects (row of that event, value). Clicking a column header selects the
  value's first event.
- Selection shows as a ring on the cell, a tint on its row and column, and a bright left edge on
  the row.
- The filter is a substring match on value name or type, and hides non-matching columns.

Inspector, for the selected event:

- Value name and type, event kind.
- Call, access (derived from kind), source location, last call that moved the value.
- Call path: the ancestor chain from the root to the call.
- Lifetime: every event on that value in order; each entry selects that event.

## Limits

- **Moves are syntactic.** With no borrow checker, a move in one branch counts on every path,
  and a partial move (`x.field`) is a move of nothing.
- **Trait calls stop at the trait.** A call through `dyn Trait` or a type parameter resolves to
  the trait's method, which has no body to inline.
- **Proc-macro output is unseen**: derives, and what an attribute macro wraps a fn in
  (`#[tokio::main]`).
- **One cfg set.** rust-analyzer analyzes the host's configuration with default features; code
  cfg'd out is not there. Core's own counts cover all code.
- **Stored closures are not followed.** A closure bound to a local and called later is a row,
  but its body is listed where it is written.
- **Crossing crates needs both reports loaded**; otherwise the call is a leaf.
- **Columns grow with depth.** Every binding of every inlined fn is a column; the filter and the
  depth cap are the only relief.

## Open

- Whether the value analysis is always on, or behind a flag or cargo feature.
- Whether to track every binding or leave out `Copy` scalars.
