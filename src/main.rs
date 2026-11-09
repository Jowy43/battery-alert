#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hid;
mod tray;

fn main() {
    if let Err(error) = tray::run() {
        eprintln!("Battery Alert no pudo iniciarse: {error}");
    }
}
