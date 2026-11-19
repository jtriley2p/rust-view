//! Embeds the built dashboard (`web/dist/index.html`), or a stub when it has not been built.

use std::path::Path;
use std::{env, fs};

const STUB: &str = "<!doctype html>
<meta charset=\"utf-8\">
<title>rust-view</title>
<p>The dashboard was not built into this binary. Run <code>bun run build</code> in <code>web/</code>, then rebuild the CLI.</p>
";

fn main() {
    let page = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist/index.html");
    println!("cargo::rerun-if-changed={}", page.display());
    let html = fs::read_to_string(&page).unwrap_or_else(|_| STUB.into());
    let out = Path::new(&env::var_os("OUT_DIR").unwrap_or_default()).join("index.html");
    // Left alone when unchanged, so a missing build does not recompile the CLI every time.
    if fs::read_to_string(&out).is_ok_and(|old| old == html) {
        return;
    }
    if let Err(e) = fs::write(&out, html) {
        println!("cargo::error={}: {e}", out.display());
    }
}
