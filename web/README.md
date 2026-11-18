# rust-view web

Dashboard for the reports the `rust-view` CLI writes (schema: `crates/core/spec/report.schema.json`).

```sh
bun install
bun dev          # dev server with reload
bun run build    # static bundle in dist/
bun run check    # type-check
```

Generate reports with `rust-view -o reports` in a cargo project, then open the files from the sidebar or drop them on
the page. A workspace writes one report per crate; open them together and they are merged. The stdout form (one report
per line) loads too.

- `src/report.ts`: report types, validation, merging.
- `src/overview.tsx`: the overview screen.
- `src/items.tsx`: the module items screen: a module tree where a module opens to list its items. Focusing an item
  draws arrows from it to the modules that import it and on from those that re-export it.
- `src/flow.tsx`: the type flow screen: the report's flows as a graph of types and the functions between them, sliced
  to what is within a few hops of one node and boxed by module. An arrow's colour and head give how a type is taken or
  given (move, read, mutate, convert); a dashed arrow ends at an error, and bars across one mark a channel. Selecting
  a node centres the slice on it.
- `src/dependencies.tsx`: the external dependencies screen, a layered graph laid out by `elkjs`.
- `src/canvas.tsx`: the zooming, panning SVG canvas those three draw in.
- `flint/`: the Flint design system's runtime files (tokens, fonts, component bundle). Styles in `src/app.css` use its
  tokens only.
