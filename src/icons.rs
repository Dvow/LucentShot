use crate::draw::Tool;
use eframe::egui::{self, Color32, ColorImage, TextureHandle};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Pen,
    Line,
    Arrow,
    Square,
    SquareFill,
    Marker,
    Text,
    Undo,
    Cloud,
    Google,
    #[cfg(feature = "ocr")]
    Ocr,
    #[cfg(feature = "ocr")]
    Speak,
    Print,
    Save,
    Copy,
    Close,
}

const BTN: f32 = 18.0;

pub fn tool_toggle(ui: &mut egui::Ui, current: &mut Tool, tool: Tool, icon: Icon, tip: &str) {
    if icon_selectable(ui, *current == tool, icon, tip) {
        *current = tool;
    }
    ui.add_space(2.0);
}

pub fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str) -> bool {
    icon_selectable(ui, false, icon, tip)
}

pub fn icon_selectable(ui: &mut egui::Ui, selected: bool, icon: Icon, tip: &str) -> bool {
    let texture = icon_texture(ui.ctx(), icon);
    let image = egui::Image::new((texture.id(), egui::vec2(14.0, 14.0)));
    ui.add_sized(
        [BTN, BTN],
        egui::Button::image(image)
            .selected(selected)
            .frame_when_inactive(selected),
    )
    .on_hover_text(tip)
    .clicked()
}

fn icon_texture(ctx: &egui::Context, icon: Icon) -> TextureHandle {
    let px = ((BTN * ctx.pixels_per_point()).round() as u32).clamp(18, 96);
    let id = egui::Id::new(("lucide_icon", icon as u8, px));
    if let Some(texture) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return texture;
    }
    let image = rasterize(icon.svg(), px);
    let texture = ctx.load_texture(
        format!("lucide-{px}-{}", icon as u8),
        image,
        Default::default(),
    );
    ctx.data_mut(|d| d.insert_temp(id, texture.clone()));
    texture
}

fn rasterize(svg: &str, px: u32) -> ColorImage {
    let empty = ColorImage::filled([px as usize, px as usize], Color32::TRANSPARENT);
    let svg = svg.replace("currentColor", "#ffffff");
    let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()) else {
        return empty;
    };
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(px, px) else {
        return empty;
    };
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return empty;
    }
    let transform = resvg::tiny_skia::Transform::from_scale(
        px as f32 / size.width(),
        px as f32 / size.height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    ColorImage::new(
        [px as usize, px as usize],
        pixmap
            .pixels()
            .iter()
            .map(|p| Color32::from_rgba_premultiplied(p.red(), p.green(), p.blue(), p.alpha()))
            .collect(),
    )
}

impl Icon {
    fn svg(self) -> &'static str {
        match self {
            Self::Pen => include_str!("../assets/icons/pencil.svg"),
            Self::Line => include_str!("../assets/icons/line.svg"),
            Self::Arrow => include_str!("../assets/icons/arrow-right.svg"),
            Self::Square => include_str!("../assets/icons/square.svg"),
            Self::SquareFill => include_str!("../assets/icons/square-fill.svg"),
            Self::Marker => include_str!("../assets/icons/highlighter.svg"),
            Self::Text => include_str!("../assets/icons/type.svg"),
            Self::Undo => include_str!("../assets/icons/undo-2.svg"),
            Self::Cloud => include_str!("../assets/icons/cloud-upload.svg"),
            Self::Google => include_str!("../assets/icons/search.svg"),
            #[cfg(feature = "ocr")]
            Self::Ocr => include_str!("../assets/icons/align-left.svg"),
            #[cfg(feature = "ocr")]
            Self::Speak => include_str!("../assets/icons/volume-2.svg"),
            Self::Print => include_str!("../assets/icons/printer.svg"),
            Self::Save => include_str!("../assets/icons/save.svg"),
            Self::Copy => include_str!("../assets/icons/copy.svg"),
            Self::Close => include_str!("../assets/icons/x.svg"),
        }
    }
}
