# Pinned Unicode16 table generation

The adjacent `../unicode-16.0.0` directory contains the required Unicode16 inputs and their SHA256SUMS/license. Inputs originate from:

- https://www.unicode.org/Public/zipped/16.0.0/UCD.zip (SHA256 c86dd81f2b14a43b0cc064aa5f89aa7241386801e35c59c7984e579832634eb2)
- https://www.unicode.org/Public/emoji/16.0/emoji-sequences.txt
- https://www.unicode.org/Public/emoji/16.0/emoji-zwj-sequences.txt

Cargo excludes nested packages from crate archives. The generator manifest is therefore retained as `Cargo.toml.generator`. In an extracted working copy, copy it to `Cargo.toml`; from this directory run `cargo run --locked`, then format `../src/unicodetables.rs` and `../tests/unicode_property_escapes.rs` with the repository Rust formatter. The generated source targets pinned Node24.2.0 Unicode16 behavior. Do not replace inputs with latest Unicode data silently. No download occurs during ordinary builds.
