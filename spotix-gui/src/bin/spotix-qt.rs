#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    spotix_gui::qt::launcher::run();
}
