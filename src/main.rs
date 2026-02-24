#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod db;
mod models;

use std::path::PathBuf;

use app::SystemsCatalogApp;
use db::Repository;
use eframe::egui::{Pos2, Vec2, ViewportBuilder};

fn read_f32_setting(repository: &Repository, key: &str) -> Option<f32> {
    repository
        .get_setting(key)
        .ok()
        .flatten()
        .and_then(|value| value.parse::<f32>().ok())
}

fn main() -> eframe::Result<()> {
    let database_path = PathBuf::from("systems_catalog.db");

    let repository = Repository::open(&database_path)
        .expect("failed to open SQLite database for Systems Catalog");

    let window_width = read_f32_setting(&repository, "window_width").unwrap_or(1280.0);
    let window_height = read_f32_setting(&repository, "window_height").unwrap_or(820.0);
    let window_x = read_f32_setting(&repository, "window_x");
    let window_y = read_f32_setting(&repository, "window_y");

    let mut viewport = ViewportBuilder::default()
        .with_inner_size(Vec2::new(window_width.max(640.0), window_height.max(480.0)));

    if let (Some(x), Some(y)) = (window_x, window_y) {
        viewport = viewport.with_position(Pos2::new(x, y));
    }

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Systems Catalog",
        native_options,
        Box::new(move |creation_context| {
            creation_context
                .egui_ctx
                .set_visuals(eframe::egui::Visuals::dark());

            let app = SystemsCatalogApp::new(repository)
                .expect("failed to initialize Systems Catalog application state");

            Box::new(app)
        }),
    )
}
