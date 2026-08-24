use ab_glyph::{FontRef, PxScale};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use image::{DynamicImage, Rgba};
use imageproc::drawing::draw_text_mut;
use std::sync::OnceLock;
use resvg::tiny_skia::{self, Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap};

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Pen,
    Line,
    Arrow,
    Rect,
    Marker,
    Text,
}

#[derive(Clone)]
pub struct Shape {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub stroke_width: f32,
    pub tool: Tool,
    pub text: String,
    pub opacity: f32,
    pub rect_filled: bool,
}

impl Shape {
    pub fn stroke(
        tool: Tool,
        start: Pos2,
        color: Color32,
        marker_opacity: f32,
        rect_filled: bool,
    ) -> Self {
        let marker = tool == Tool::Marker;
        Self {
            points: vec![start],
            color,
            stroke_width: if marker { 15.0 } else { 2.5 },
            tool,
            text: String::new(),
            opacity: if marker { marker_opacity } else { 1.0 },
            rect_filled: tool == Tool::Rect && rect_filled,
        }
    }

    pub fn text_label(pos: Pos2, color: Color32) -> Self {
        Self {
            points: vec![pos],
            color,
            stroke_width: 2.0,
            tool: Tool::Text,
            text: String::new(),
            opacity: 1.0,
            rect_filled: false,
        }
    }

    pub fn is_marker(&self) -> bool {
        self.tool == Tool::Marker
    }

    fn paint_color(&self) -> Color32 {
        if self.is_marker() {
            self.color.gamma_multiply(self.opacity)
        } else {
            self.color
        }
    }
}

pub fn paint_shape(painter: &egui::Painter, shape: &Shape) {
    let color = shape.paint_color();
    let stroke = Stroke::new(shape.stroke_width, color);
    match shape.tool {
        Tool::Pen | Tool::Marker => {
            if shape.points.len() <= 1 {
                return;
            }
            let mut path_points = shape.points.clone();
            for i in (1..path_points.len().saturating_sub(1)).rev() {
                let p1 = path_points[i];
                let p2 = path_points[i + 1];
                path_points[i] = egui::pos2((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
            }
            painter.add(egui::epaint::Shape::Path(egui::epaint::PathShape {
                points: path_points,
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: stroke.into(),
            }));
        }
        Tool::Line => {
            let (Some(&start), Some(&end)) = (shape.points.first(), shape.points.last()) else {
                return;
            };
            painter.line_segment([start, end], stroke);
        }
        Tool::Rect => {
            let (Some(&start), Some(&end)) = (shape.points.first(), shape.points.last()) else {
                return;
            };
            let rect = Rect::from_two_pos(start, end);
            if shape.rect_filled {
                painter.rect_filled(rect, 0.0, shape.color);
            } else {
                painter.rect_stroke(rect, 0.0, stroke, egui::StrokeKind::Middle);
            }
        }
        Tool::Arrow => {
            if let Some(arrow) = arrow_geometry(
                shape.points.first().copied(),
                shape.points.last().copied(),
                1.0,
            ) {
                painter.line_segment([arrow.start, arrow.shaft_end], stroke);
                painter.add(egui::epaint::Shape::convex_polygon(
                    vec![arrow.tip, arrow.left, arrow.right],
                    color,
                    Stroke::NONE,
                ));
                painter.line_segment([arrow.tip, arrow.left], stroke);
                painter.line_segment([arrow.tip, arrow.right], stroke);
            }
        }
        Tool::Text => {
            let Some(pos) = shape.points.first() else {
                return;
            };
            painter.text(
                *pos,
                egui::Align2::LEFT_TOP,
                &shape.text,
                egui::FontId::proportional(20.0),
                color,
            );
        }
    }
}

pub fn rasterize_and_crop(
    full_img: &DynamicImage,
    shapes: &[Shape],
    selection: Rect,
    ppp: f32,
) -> DynamicImage {
    let rgba_img = full_img
        .as_rgba8()
        .expect("captured screenshot is always RGBA8");
    let (phys_w, phys_h) = rgba_img.dimensions();

    let min_x = selection.min.x.min(selection.max.x);
    let min_y = selection.min.y.min(selection.max.y);
    let max_x = selection.min.x.max(selection.max.x);
    let max_y = selection.min.y.max(selection.max.y);

    let crop_x = ((min_x * ppp).round() as u32).min(phys_w.saturating_sub(1));
    let crop_y = ((min_y * ppp).round() as u32).min(phys_h.saturating_sub(1));
    let crop_w = (((max_x - min_x) * ppp).round().max(1.0) as u32)
        .min(phys_w.saturating_sub(crop_x))
        .max(1);
    let crop_h = (((max_y - min_y) * ppp).round().max(1.0) as u32)
        .min(phys_h.saturating_sub(crop_y))
        .max(1);

    let mut pixmap = Pixmap::new(phys_w, phys_h).expect("screenshot dimensions are non-zero");
    pixmap.data_mut().copy_from_slice(rgba_img.as_raw());

    for shape in shapes {
        rasterize_shape(&mut pixmap, shape, ppp);
    }

    let pixels = pixmap.take();
    let mut full = DynamicImage::ImageRgba8(
        image::ImageBuffer::from_raw(phys_w, phys_h, pixels)
            .expect("pixmap size matches RGBA buffer"),
    );

    for shape in shapes {
        if shape.tool != Tool::Text {
            continue;
        }
        let Some(pos) = shape.points.first() else {
            continue;
        };
        let Some(font) = system_ui_font() else {
            continue;
        };
        let color = Rgba([
            shape.color.r(),
            shape.color.g(),
            shape.color.b(),
            shape.color.a(),
        ]);
        draw_text_mut(
            &mut full,
            color,
            (pos.x * ppp) as i32,
            (pos.y * ppp) as i32,
            PxScale {
                x: 20.0 * ppp,
                y: 20.0 * ppp,
            },
            font,
            &shape.text,
        );
    }

    full.crop_imm(crop_x, crop_y, crop_w, crop_h)
}

fn rasterize_shape(pixmap: &mut Pixmap, shape: &Shape, ppp: f32) {
    let mut paint = Paint::default();
    let alpha = if shape.is_marker() {
        (shape.opacity * 255.0) as u8
    } else {
        shape.color.a()
    };
    paint.set_color(Color::from_rgba8(
        shape.color.r(),
        shape.color.g(),
        shape.color.b(),
        alpha,
    ));
    paint.anti_alias = true;

    let stroke = tiny_skia::Stroke {
        width: shape.stroke_width * ppp,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    let transform = tiny_skia::Transform::identity();

    match shape.tool {
        Tool::Pen | Tool::Marker => {
            if shape.points.len() <= 1 {
                return;
            }
            let mut pb = PathBuilder::new();
            let p0 = shape.points[0];
            pb.move_to(p0.x * ppp, p0.y * ppp);
            if shape.points.len() == 2 {
                pb.line_to(shape.points[1].x * ppp, shape.points[1].y * ppp);
            } else {
                for i in 1..shape.points.len() - 1 {
                    let p1 = shape.points[i];
                    let p2 = shape.points[i + 1];
                    pb.quad_to(
                        p1.x * ppp,
                        p1.y * ppp,
                        ((p1.x + p2.x) / 2.0) * ppp,
                        ((p1.y + p2.y) / 2.0) * ppp,
                    );
                }
                if let Some(last) = shape.points.last() {
                    pb.line_to(last.x * ppp, last.y * ppp);
                }
            }
            let Some(path) = pb.finish() else {
                return;
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
        Tool::Line => {
            let (Some(p1), Some(p2)) = (shape.points.first(), shape.points.last()) else {
                return;
            };
            let mut pb = PathBuilder::new();
            pb.move_to(p1.x * ppp, p1.y * ppp);
            pb.line_to(p2.x * ppp, p2.y * ppp);
            let Some(path) = pb.finish() else {
                return;
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
        Tool::Rect => {
            let (Some(p1), Some(p2)) = (shape.points.first(), shape.points.last()) else {
                return;
            };
            let rx = (p1.x * ppp).min(p2.x * ppp);
            let ry = (p1.y * ppp).min(p2.y * ppp);
            let rw = (p1.x * ppp - p2.x * ppp).abs();
            let rh = (p1.y * ppp - p2.y * ppp).abs();
            let Some(rect) = tiny_skia::Rect::from_xywh(rx, ry, rw, rh) else {
                return;
            };
            let path = PathBuilder::from_rect(rect);
            if shape.rect_filled {
                pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
            } else {
                pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            }
        }
        Tool::Arrow => {
            if let Some(arrow) = arrow_geometry(
                shape.points.first().copied(),
                shape.points.last().copied(),
                ppp,
            ) {
                let mut pb = PathBuilder::new();
                pb.move_to(arrow.start.x, arrow.start.y);
                pb.line_to(arrow.shaft_end.x, arrow.shaft_end.y);
                pb.move_to(arrow.tip.x, arrow.tip.y);
                pb.line_to(arrow.left.x, arrow.left.y);
                pb.move_to(arrow.tip.x, arrow.tip.y);
                pb.line_to(arrow.right.x, arrow.right.y);

                let mut head = PathBuilder::new();
                head.move_to(arrow.tip.x, arrow.tip.y);
                head.line_to(arrow.left.x, arrow.left.y);
                head.line_to(arrow.right.x, arrow.right.y);
                head.close();
                if let Some(head_path) = head.finish() {
                    pixmap.fill_path(&head_path, &paint, FillRule::Winding, transform, None);
                }
                let Some(path) = pb.finish() else {
                    return;
                };
                pixmap.stroke_path(&path, &paint, &stroke, transform, None);
            }
        }
        Tool::Text => {}
    }
}

struct ArrowGeometry {
    start: Pos2,
    shaft_end: Pos2,
    tip: Pos2,
    left: Pos2,
    right: Pos2,
}

fn arrow_geometry(start: Option<Pos2>, end: Option<Pos2>, ppp: f32) -> Option<ArrowGeometry> {
    let start = start?;
    let end = end?;
    let x1 = start.x * ppp;
    let y1 = start.y * ppp;
    let x2 = end.x * ppp;
    let y2 = end.y * ppp;
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 0.0 {
        return None;
    }
    let ux = dx / len;
    let uy = dy / len;
    let head_len = 14.4 * ppp;
    let head_wid = 8.4 * ppp;
    Some(ArrowGeometry {
        start: Pos2::new(x1, y1),
        shaft_end: Pos2::new(x2 - ux * head_len, y2 - uy * head_len),
        tip: Pos2::new(x2, y2),
        left: Pos2::new(
            x2 - ux * head_len - uy * (head_wid * 0.5),
            y2 - uy * head_len + ux * (head_wid * 0.5),
        ),
        right: Pos2::new(
            x2 - ux * head_len + uy * (head_wid * 0.5),
            y2 - uy * head_len - ux * (head_wid * 0.5),
        ),
    })
}

fn system_ui_font() -> Option<&'static FontRef<'static>> {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    static FONT: OnceLock<Option<FontRef<'static>>> = OnceLock::new();
    FONT.get_or_init(|| {
        let bytes = BYTES.get_or_init(load_system_ui_font);
        if bytes.is_empty() {
            return None;
        }
        FontRef::try_from_slice(bytes).ok()
    })
    .as_ref()
}

fn load_system_ui_font() -> Vec<u8> {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let fonts = std::path::Path::new(&windir).join("Fonts");
    for name in ["segoeui.ttf", "calibri.ttf", "arial.ttf", "tahoma.ttf"] {
        if let Ok(bytes) = std::fs::read(fonts.join(name)) {
            return bytes;
        }
    }
    Vec::new()
}
