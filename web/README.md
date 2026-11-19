# rust-view web

Dashboard for the reports the `rust-view` CLI writes (schema: `crates/core/spec/report.schema.json`).

```sh
bun install
bun dev          # dev server with reload
bun run build    # the whole dashboard as one file, dist/index.html
bun run check    # type-check
```

The CLI embeds `dist/index.html` when it is compiled, so build the dashboard first: `bun run build && cargo build`.
A CLI compiled without it installs a page that says so.

`rust-view <path>` keeps a project's reports in the store, `~/.rust-view`, and `rust-view --open` installs the
dashboard there and opens it:

```
~/.rust-view/
  index.html          # the dashboard, rewritten by each run
  index.js            # rustView.index([{ id, name, path, time, version, crates }, ...]), oldest first
  reports/<id>.js     # rustView.report("<id>", [one report per crate])
```

The store is scripts because a `file://` page can run a script beside it but cannot fetch or list files. The page
opens on the newest project, or on the one picked last if nothing has been written since; "Open reports" lists them
all. Running a project again replaces its entry, and reloading the page shows it.

Under `bun dev` there is no store: drop report files on the page (`rust-view <path> -o reports`, or the one-per-line
form of `-o -`). A workspace writes one report per crate; dropped together they are merged.

- `src/store.ts`: reads the store.
- `src/report.ts`: report types, validation, merging.
- `src/overview.tsx`: the overview screen.
- `src/items.tsx`: the module items screen: a module tree where a module opens to list its items. Focusing an item
  draws arrows from it to the modules that import it and on from those that re-export it.
- `src/flow.tsx`: the type flow screen: the report's flows as a graph of types and the functions between them, sliced
  to what is within a few hops of one node and boxed by module. An arrow's colour and head give how a type is taken or
  given (move, read, mutate, convert); a dashed arrow ends at an error, and bars across one mark a channel. The legend's
  switches hide a kind or a mark, and a walk does not cross a hidden arrow. Selecting
  a node centres the slice on it; pinning one keeps it and its own slice drawn, and the node where it is on screen, as
  the focus moves.
- `src/dependencies.tsx`: the external dependencies screen, a layered graph laid out by `elkjs`.
- `src/canvas.tsx`: the zooming, panning SVG canvas those three draw in. It can hold one drawing point still on screen.
- `flint/`: the Flint design system's runtime files (tokens, fonts, component bundle). Styles in `src/app.css` use its
  tokens only.
