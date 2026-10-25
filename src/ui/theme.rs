use eframe::egui::{self, Color32};

pub const BACKGROUND: Color32 = Color32::from_rgb(17, 18, 23);
pub const SURFACE: Color32 = Color32::from_rgb(25, 27, 34);
pub const FOREGROUND: Color32 = Color32::from_rgb(236, 237, 243);
pub const ACCENT: Color32 = Color32::from_rgb(175, 166, 255);
pub const MUTED: Color32 = Color32::from_rgb(161, 167, 184);
pub const SELECTED: Color32 = Color32::from_rgb(60, 52, 96);

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(12, 13, 18);
    visuals.override_text_color = Some(FOREGROUND);
    visuals.selection.bg_fill = SELECTED;
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, FOREGROUND);
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, Color32::from_rgb(49, 52, 65));
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(43, 45, 59);
    visuals.widgets.active.bg_fill = SELECTED;
    for widget in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.corner_radius = egui::CornerRadius::same(8);
    }
    ctx.set_visuals(visuals);
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(12.0, 12.0);
        s.spacing.button_padding = egui::vec2(16.0, 10.0);
        s.spacing.window_margin = egui::Margin::same(20);
        s.text_styles
            .insert(egui::TextStyle::Heading, egui::FontId::proportional(30.0));
        s.text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
        s.text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(14.0));
        s.text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
    });
}

pub fn logo() -> egui::ColorImage {
    let svg =
        crate::svg_logo::portable(include_str!("../../assets/logo.svg")).expect("bundled SVG logo");
    let image = crate::svg_logo::render(&svg, 256).expect("native SVG rendering");
    egui::ColorImage::from_rgba_premultiplied([256, 256], image.data())
}

pub fn icon() -> egui::IconData {
    let svg =
        crate::svg_logo::portable(include_str!("../../assets/logo.svg")).expect("bundled SVG logo");
    let png = crate::svg_logo::render(&svg, 128)
        .expect("SVG icon")
        .encode_png()
        .expect("icon encoding");
    let image = image::load_from_memory(&png)
        .expect("rendered SVG icon")
        .into_rgba8();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn supplied_logo_decodes_and_icon_is_usable() {
        let logo = super::logo();
        assert_eq!(logo.size, [256, 256]);
        let icon = super::icon();
        assert_eq!(icon.rgba.len(), 128 * 128 * 4);
        assert!(logo.pixels.iter().any(|p| p.r() > 100 && p.g() < 60));
        assert!(logo.pixels.iter().any(|p| p.g() > 100 && p.r() < 100));
        assert_eq!(
            &include_bytes!(concat!(env!("OUT_DIR"), "/app.ico"))[..4],
            &[0, 0, 1, 0]
        );
    }
}
