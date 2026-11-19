# rust-view

cargo project metrics-gen cli & viz dasboard

## structure

- **core** (`crates/core`): pure analyzer. src text in, `Report` + errs out. no fs, processes, or manifest parsing
- **cli** (`crates/cli`): `rust-view` bin. handles io (`cargo metadata`, read src, write reports) & feeds core
  - keeps reports per project in the store (`~/.rust-view`, or `$RUST_VIEW_HOME`) & embeds the built dashboard
- **web** (`web/`): bun+react dashboard that loads reports from the store. styled w flint design system

report schema (`crates/core/spec/`) is the contract between each

`crates/core/doc/core.md` holds design decisions, rules, & known limits; read before changing analysis behavior & update on change

## cmd

```sh
# run tests
cargo test
# gen reports into the store & open the dashboard (build web first, the cli embeds it)
(cd web && bun run build) && cargo run -p rust-view-cli -- . --open
# gen plain json reports
cargo run -p rust-view-cli -- . -o reports/
# run dashboard dev server (no store; drop report files on the page)
cd web && bun dev
# type check
bun run check
```

## rules

- core stays pure; external interactions are in the cli
- report shape change touches schema, rs types, web types, & the example
- core never panics; errors are enumerated
  - only tests may panic
- minimize deps, esp in core
- keep it lightweight & minimalist
