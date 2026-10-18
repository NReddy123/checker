mod app;
mod diff;
// Some editor commands are reserved for the aligned keyboard-editing path.
#[allow(dead_code)]
mod diff_editor;
mod folder;
mod session;
mod theme;
mod ui;

fn main() {
    app::run();
}
