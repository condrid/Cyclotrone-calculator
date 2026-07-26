#![cfg_attr(feature = "bundle", windows_subsystem = "windows")]

mod app;
mod calculations;
mod components;
mod database;
mod models;
mod storage;

fn main() {
    app::launch();
}
