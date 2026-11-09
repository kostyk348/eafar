//! eafar-viz binary. The window itself lives behind the `gui` feature:
//! `cargo run --release -p viz --features gui` (requires cargo >= 1.85 —
//! parts of the egui/eframe dependency tree ship edition-2024 manifests).
//! Without the feature the crate still builds and its simulation logic
//! (rewind buffer, stats, playback) is fully unit-tested.

#[cfg(feature = "gui")]
fn main() -> eframe::Result<()> {
    viz::gui::run()
}

#[cfg(not(feature = "gui"))]
fn main() {
    eprintln!("eafar-viz: the GUI is not compiled in (feature `gui` is off).");
    eprintln!("  build/run it with: cargo run --release -p viz --features gui");
    eprintln!("  note: the `gui` feature requires cargo >= 1.85 (edition-2024 manifests in the egui dependency tree).");
    std::process::exit(2);
}
