#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod file_store;
mod models;
mod plugins;
mod project_store;

use std::path::PathBuf;

use app::SystemsCatalogApp;
use eframe::egui::{
    FontData, FontDefinitions, FontFamily, Rounding, Style, Vec2, ViewportBuilder,
};
use file_store::FileStore;

fn apply_phase6_style_tokens(context: &eframe::egui::Context) {
    let rounding = 2.0;
    let panel_rounding = Rounding::same(rounding);
    let mut visuals = eframe::egui::Visuals::dark();
    visuals.window_rounding = panel_rounding;
    visuals.menu_rounding = panel_rounding;
    visuals.panel_fill = eframe::egui::Color32::from_rgb(24, 26, 30);
    visuals.extreme_bg_color = eframe::egui::Color32::from_rgb(18, 20, 24);
    visuals.widgets.active.rounding = panel_rounding;
    visuals.widgets.hovered.rounding = panel_rounding;
    visuals.widgets.inactive.rounding = panel_rounding;
    visuals.slider_trailing_fill = true;
    
    let mut style: Style = (*context.style()).clone();
    style.spacing.item_spacing = Vec2::new(5.0, 3.0);
    style.spacing.window_margin = eframe::egui::Margin::same(8.0);

    style.spacing.button_padding = Vec2::new(15.0, 5.0);
    style.spacing.menu_margin = eframe::egui::Margin::same(8.0);
    style.visuals = visuals;
    
    

    context.set_style(style);
}

fn main() -> eframe::Result<()> {
    // Use FileStore for file-native project storage
    let project_dir = PathBuf::from(".");
    let store = FileStore::open(&project_dir)
        .unwrap_or_else(|_| FileStore::create(&project_dir)
            .expect("failed to create FileStore for Systems Catalog"));
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

            apply_phase6_style_tokens(&creation_context.egui_ctx);

            let mut app = SystemsCatalogApp::new(store)
                .expect("failed to initialize Systems Catalog application state");

            if let Some(storage) = creation_context.storage {
                if let Some(saved_state) =
                    eframe::get_value::<app::EframePersistedUiState>(storage, eframe::APP_KEY)
                {
                    app.apply_eframe_persisted_state(saved_state);
                }
            }

            Box::new(app)
        }),
    )
}
