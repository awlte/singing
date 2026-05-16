// Hide console on Windows in release builds.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

fn main() {
    singing_lib::run();
}
