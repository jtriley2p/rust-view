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
- `flint/`: the Flint design system's runtime files (tokens, fonts, component bundle). Styles in `src/app.css` use its
  tokens only.
