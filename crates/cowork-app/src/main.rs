//! Cowork Desktop Application Entry Point

// Prevents console window from appearing on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cowork_app::run();
}
