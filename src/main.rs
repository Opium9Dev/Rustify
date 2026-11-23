#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod discord;
mod i18n;
mod profile;

fn main() -> eframe::Result<()> {
    app::run()
}
