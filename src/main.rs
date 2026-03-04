#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod db;
mod models;
mod plugins;
mod project_store;

use std::path::PathBuf;

use app::SystemsCatalogApp;
use db::Repository;
use eframe::egui::{FontData, FontDefinitions, FontFamily, Vec2, ViewportBuilder};

fn main() -> eframe::Result<()> {
    let database_path = PathBuf::from("systems_catalog.db");

    // not recommended practice:
    // docs say to prefer explicitly handling errors instead of using `expect` or `unwrap` in production code
    // docs also specify that the error message for `expect`, should be the reason it 'should' succeed, " env variable `IMPORTANT_PATH` should be set by `wrapper_script.sh "
    let repository = Repository::open(&database_path)
        .expect("failed to open SQLite database for Systems Catalog");
    let viewport = ViewportBuilder::default().with_inner_size(Vec2::new(1280.0, 820.0));

    let native_options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Systems Catalog",
        native_options,
        Box::new(move |creation_context| {
            let mut fonts = FontDefinitions::default();
            fonts.font_data.insert(
                "material_icons".to_owned(),
                FontData::from_static(egui_material_icons::FONT_DATA),
            );
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("material_icons".to_owned());
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("material_icons".to_owned());
            creation_context.egui_ctx.set_fonts(fonts);

            creation_context
                .egui_ctx
                .set_visuals(eframe::egui::Visuals::dark());

            let app = SystemsCatalogApp::new(repository)
                .expect("failed to initialize Systems Catalog application state");

            Box::new(app)
        }),
    )
}
