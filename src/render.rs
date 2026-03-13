use image::{DynamicImage, Rgba};
use std::sync::OnceLock;
use tiny_skia::{Color, Paint, PathBuilder, Pixmap, Stroke, LineCap, LineJoin, FillRule};
use imageproc::drawing::draw_text_mut;
use rusttype::{Font, Scale};
use crate::config::{Shape, Tool};

fn font() -> &'static Font<'static> {
    static FONT: OnceLock<Font<'static>> = OnceLock::new();
    FONT.get_or_init(|| {
        #[cfg(windows)]
        let path = std::path::Path::new(r"C:\Windows\Fonts\arial.ttf");
        #[cfg(not(windows))]
        let path = std::path::Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf");
        let font_data = std::fs::read(path).expect("Failed to load Arial font");
        let leaked: &'static [u8] = Box::leak(font_data.into_boxed_slice());
        Font::try_from_bytes(leaked).expect("Error constructing Font")
    })
}

pub fn render_and_crop(full_img: &DynamicImage, shapes: &[Shape], selection: eframe::egui::Rect, ppp: f32) -> DynamicImage {
    let rgba_img_base = full_img.as_rgba8().unwrap();
    let (phys_w, phys_h) = rgba_img_base.dimensions();

    let min_x = selection.min.x.min(selection.max.x);
    let min_y = selection.min.y.min(selection.max.y);
    let max_x = selection.min.x.max(selection.max.x);
    let max_y = selection.min.y.max(selection.max.y);

    let crop_x = (min_x * ppp).round() as u32;
    let crop_y = (min_y * ppp).round() as u32;
    let crop_w = ((max_x - min_x) * ppp).round() as u32;
    let crop_h = ((max_y - min_y) * ppp).round() as u32;

    let crop_x = crop_x.min(phys_w.saturating_sub(1));
    let crop_y = crop_y.min(phys_h.saturating_sub(1));
    let crop_w = crop_w.min(phys_w.saturating_sub(crop_x)).max(1);
    let crop_h = crop_h.min(phys_h.saturating_sub(crop_y)).max(1);

    let mut pixmap = Pixmap::new(phys_w, phys_h).unwrap();
    pixmap.data_mut().copy_from_slice(rgba_img_base.as_raw());

    for shape in shapes {
        let mut paint = Paint::default();
        
        if shape.is_marker {
            // Use standard alpha blending instead of Multiply to prevent darkening on dark backgrounds.
            // This matches the UI preview's look perfectly.
            let skia_color = Color::from_rgba8(
                shape.color.r(),
                shape.color.g(),
                shape.color.b(),
                (shape.opacity * 255.0) as u8,
            );
            paint.set_color(skia_color);
        } else {
            let skia_color = Color::from_rgba8(
                shape.color.r(),
                shape.color.g(),
                shape.color.b(),
                shape.color.a()
            );
            paint.set_color(skia_color);
        }
        
        paint.anti_alias = true;

        let mut stroke = Stroke::default();
        stroke.width = shape.stroke_width * ppp;
        stroke.line_cap = LineCap::Round;
        stroke.line_join = LineJoin::Round;

        let mut pb = PathBuilder::new();

        match shape.tool {
            Tool::Pen | Tool::Marker => {
                if shape.points.len() > 1 {
                    let p0 = shape.points[0];
                    pb.move_to(p0.x * ppp, p0.y * ppp);
                    
                    if shape.points.len() == 2 {
                        pb.line_to(shape.points[1].x * ppp, shape.points[1].y * ppp);
                    } else {
                        for i in 1..shape.points.len() - 1 {
                            let p1 = shape.points[i];
                            let p2 = shape.points[i + 1];
                            let mid_x = (p1.x + p2.x) / 2.0;
                            let mid_y = (p1.y + p2.y) / 2.0;
                            pb.quad_to(p1.x * ppp, p1.y * ppp, mid_x * ppp, mid_y * ppp);
                        }
                        if let Some(last) = shape.points.last() {
                            pb.line_to(last.x * ppp, last.y * ppp);
                        }
                    }
                    
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
                    }
                }
            }
            Tool::Line => {
                if let (Some(p1), Some(p2)) = (shape.points.first(), shape.points.last()) {
                    pb.move_to(p1.x * ppp, p1.y * ppp);
                    pb.line_to(p2.x * ppp, p2.y * ppp);
                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
                    }
                }
            }
            Tool::Rect => {
                if let (Some(p1), Some(p2)) = (shape.points.first(), shape.points.last()) {
                    let rx = (p1.x * ppp).min(p2.x * ppp);
                    let ry = (p1.y * ppp).min(p2.y * ppp);
                    let rw = (p1.x * ppp - p2.x * ppp).abs();
                    let rh = (p1.y * ppp - p2.y * ppp).abs();
                    if let Some(rect) = tiny_skia::Rect::from_xywh(rx, ry, rw, rh) {
                        let path = PathBuilder::from_rect(rect);
                        pixmap.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
                    }
                }
            }
            Tool::Arrow => {
                if let (Some(p1), Some(p2)) = (shape.points.first(), shape.points.last()) {
                    let x1 = p1.x * ppp;
                    let y1 = p1.y * ppp;
                    let x2 = p2.x * ppp;
                    let y2 = p2.y * ppp;

                    let dx = x2 - x1;
                    let dy = y2 - y1;
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > 0.0 {
                        let ux = dx / len;
                        let uy = dy / len;
                        let head_len = 14.4 * ppp;
                        let head_wid = 8.4 * ppp;

                        let shaft_end_x = x2 - ux * head_len;
                        let shaft_end_y = y2 - uy * head_len;
                        pb.move_to(x1, y1);
                        pb.line_to(shaft_end_x, shaft_end_y);

                        let left_x = x2 - ux * head_len - uy * (head_wid * 0.5);
                        let left_y = y2 - uy * head_len + ux * (head_wid * 0.5);
                        let right_x = x2 - ux * head_len + uy * (head_wid * 0.5);
                        let right_y = y2 - uy * head_len - ux * (head_wid * 0.5);

                        let mut head_pb = PathBuilder::new();
                        head_pb.move_to(x2, y2);
                        head_pb.line_to(left_x, left_y);
                        head_pb.line_to(right_x, right_y);
                        head_pb.close();
                        if let Some(head_path) = head_pb.finish() {
                            pixmap.fill_path(&head_path, &paint, FillRule::Winding, tiny_skia::Transform::identity(), None);
                        }

                        pb.move_to(x2, y2);
                        pb.line_to(left_x, left_y);
                        pb.move_to(x2, y2);
                        pb.line_to(right_x, right_y);
                    }

                    if let Some(path) = pb.finish() {
                        pixmap.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
                    }
                }
            }
            Tool::Text => {}
        }
    }

    let final_rgba = pixmap.take();
    let mut final_full = DynamicImage::ImageRgba8(image::ImageBuffer::from_raw(phys_w, phys_h, final_rgba).unwrap());
    
    // Add text using imageproc
    for shape in shapes {
        if shape.tool == Tool::Text {
            if let Some(pos) = shape.points.first() {
                let color = Rgba([shape.color.r(), shape.color.g(), shape.color.b(), shape.color.a()]);
                let scale = Scale { x: 20.0 * ppp, y: 20.0 * ppp };
                draw_text_mut(
                    &mut final_full,
                    color,
                    (pos.x * ppp) as i32,
                    (pos.y * ppp) as i32,
                    scale,
                    font(),
                    &shape.text
                );
            }
        }
    }

    final_full.crop_imm(crop_x, crop_y, crop_w, crop_h)
}
