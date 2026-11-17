# rust-view

cargo project metrics-gen cli & viz dasboard

## structure

- **core** (`crates/core`): pure analyzer. src text in, `Report` + errs out. no fs, processes, or manifest parsing
- **cli** (`crates/cli`): `rust-view` bin. handles io (`cargo metadata`, read src, write json) & feeds core
- **web** (`web/`): bun+react dashboard that loads reports. styled w flint design system

report schema (`crates/core/spec/`) is the contract between each

`crates/core/doc/core.md` holds design decisions, rules, & known limits; read before changing analysis behavior & update on change

## cmd

```sh
# run tests
cargo test
# gen reports
cargo run -p rust-view-cli -- -o reports/
# run dashboard
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
