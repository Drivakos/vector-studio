# Vector Studio

Native vector illustration app in Rust (egui + Vello) for macOS. Code is in `vector-studio/`.

## Rules

- **Every feature and every change ships with an end-to-end test** in `vector-studio/app/tests/e2e/`, plus unit tests where they fit. See `vector-studio/TESTING.md` for the harness, the naming rules for accessible names, and the coverage table to update.
- Before calling work done: `cargo test --workspace` and `cargo clippy --workspace --all-targets`, both clean.
- Cargo needs `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`.
