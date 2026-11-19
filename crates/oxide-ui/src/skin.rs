//! Original physically rendered controls; interaction and markings remain native.
use eframe::egui::{self, Color32, Rect, TextureHandle};

pub(super) struct Skin {
    pub rotary: TextureHandle,
    pub pad_up: TextureHandle,
    pub pad_down: TextureHandle,
}
impl Skin {
    pub fn new(ctx: &egui::Context) -> image::ImageResult<Self> {
        fn load(
            ctx: &egui::Context,
            name: &str,
            bytes: &[u8],
        ) -> image::ImageResult<TextureHandle> {
            let pixels =
                image::load_from_memory_with_format(bytes, image::ImageFormat::Png)?.to_rgba8();
            let size = [pixels.width() as usize, pixels.height() as usize];
            Ok(ctx.load_texture(
                name,
                egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_raw()),
                egui::TextureOptions::LINEAR,
            ))
        }
        Ok(Self {
            rotary: load(
                ctx,
                "rotary",
                include_bytes!("../assets/hardware/rotary.png"),
            )?,
            pad_up: load(
                ctx,
                "pad-up",
                include_bytes!("../assets/hardware/pad-up.png"),
            )?,
            pad_down: load(
                ctx,
                "pad-down",
                include_bytes!("../assets/hardware/pad-down.png"),
            )?,
        })
    }
    pub fn paint(p: &egui::Painter, texture: &TextureHandle, rect: Rect) {
        p.image(
            texture.id(),
            rect,
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}
