mod app;
mod db;
mod models;

use std::path::PathBuf;

use app::SystemsCatalogApp;
use db::Repository;

fn main() -> eframe::Result<()> {
    let database_path = PathBuf::from("systems_catalog.db");

    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Systems Catalog",
        native_options,
        Box::new(move |creation_context| {
            creation_context
                .egui_ctx
                .set_visuals(eframe::egui::Visuals::dark());

            let repository = Repository::open(&database_path)
                .expect("failed to open SQLite database for Systems Catalog");

            let app = SystemsCatalogApp::new(repository)
                .expect("failed to initialize Systems Catalog application state");

            Box::new(app)
        }),
    )
}
