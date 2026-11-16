use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};
pub const BACK: Color32 = Color32::from_rgb(39, 40, 42);
pub const PANEL: Color32 = Color32::from_rgb(53, 54, 57);
pub const RAISED: Color32 = Color32::from_rgb(66, 68, 71);
pub const TEXT: Color32 = Color32::from_rgb(226, 227, 228);
pub const MUTED: Color32 = Color32::from_rgb(164, 167, 171);
pub const EDGE: Color32 = Color32::from_rgb(85, 87, 91);
pub const HILITE: Color32 = Color32::from_rgb(111, 113, 117);
pub const SHADOW: Color32 = Color32::from_rgb(22, 23, 25);
pub const WELL: Color32 = Color32::from_rgb(27, 29, 31);
pub const CONTROL: Color32 = Color32::from_rgb(76, 78, 82);
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(95, 98, 103);
pub const ACCENT: Color32 = Color32::from_rgb(216, 171, 86);
pub const COPPER: Color32 = ACCENT;
pub const METER_GREEN: Color32 = Color32::from_rgb(118, 200, 96);
pub const METER_YELLOW: Color32 = Color32::from_rgb(217, 193, 89);
pub const METER_RED: Color32 = Color32::from_rgb(229, 101, 83);
pub const TRACK: [Color32; 4] = [
    Color32::from_rgb(91, 154, 194),
    Color32::from_rgb(163, 132, 191),
    Color32::from_rgb(121, 167, 101),
    Color32::from_rgb(204, 154, 85),
];

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "studio-sans".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/LiberationSans-Regular.ttf"))
            .into(),
    );
    fonts.font_data.insert(
        "studio-mono".into(),
        egui::FontData::from_static(include_bytes!("../assets/fonts/LiberationMono-Regular.ttf"))
            .into(),
    );
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "studio-sans".into());
    fonts
        .families
        .get_mut(&FontFamily::Monospace)
        .unwrap()
        .insert(0, "studio-mono".into());
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BACK;
    style.visuals.window_fill = PANEL;
    style.visuals.extreme_bg_color = WELL;
    style.visuals.faint_bg_color = PANEL;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.5);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.inactive.bg_fill = CONTROL;
    style.visuals.widgets.inactive.weak_bg_fill = PANEL;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, SHADOW);
    style.visuals.widgets.hovered.bg_fill = CONTROL_HOVER;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, HILITE);
    style.visuals.widgets.active.bg_fill = RAISED;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, EDGE);
    style.spacing.item_spacing = egui::vec2(5.0, 5.0);
    style.spacing.button_padding = egui::vec2(8.0, 4.0);
    style.spacing.interact_size.y = 23.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(10.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(11.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(17.0));
    ctx.set_style(style);
}
