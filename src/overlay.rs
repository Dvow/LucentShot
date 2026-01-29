use eframe::egui;
use egui::{Color32, Pos2, Rect, Stroke, Id, Key};
use image::DynamicImage;
use std::sync::Arc;
use std::thread;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::menu::MenuEvent;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool { Pen, Line, Arrow, Rect, Marker, Text }

#[derive(Clone)]
pub struct Shape {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub stroke_width: f32,
    pub tool: Tool,
    pub text: String,
    pub is_marker: bool,
    pub opacity: f32,
}

#[derive(Clone)]
pub enum PendingAction {
    Copy,
    Save,
    Upload,
    OCR,
    Print {
        printer: String,
        copies: i32,
        landscape: bool,
        grayscale: bool,
        fit: bool,
        paper: String,
    },
    Google,
}

pub struct OverlayApp {
    pub screenshot: Option<Arc<DynamicImage>>,
    pub texture: Option<egui::TextureHandle>,
    pub selection: Option<Rect>,
    pub is_selecting: bool,
    pub resizing_node: Option<usize>,
    pub start_pos: Option<Pos2>,
    pub current_tool: Tool,
    pub current_color: Color32,
    pub shapes: Vec<Shape>,
    pub active_shape: Option<Shape>,
    pub is_active: bool,
    pub trigger_flag: Arc<AtomicBool>,
    pub marker_opacity: f32,
    pub editing_text_index: Option<usize>,
    pub show_print_popup: bool,
    pub cropped_preview: Option<egui::TextureHandle>,
    pub printers: Vec<String>,
    pub selected_printer: String,
    pub print_copies: i32,
    pub print_landscape: bool,
    pub print_grayscale: bool,
    pub print_paper_size: String,
    pub print_fit_to_page: bool,
    pub pending_action: Option<PendingAction>,
}

impl OverlayApp {
    pub fn new_background(trigger_flag: Arc<AtomicBool>) -> Self {
        Self {
            screenshot: None,
            texture: None,
            selection: None,
            is_selecting: false,
            resizing_node: None,
            start_pos: None,
            current_tool: Tool::Pen,
            current_color: Color32::RED,
            shapes: Vec::new(),
            active_shape: None,
            is_active: false,
            trigger_flag,
            marker_opacity: 0.4,
            editing_text_index: None,
            show_print_popup: false,
            cropped_preview: None,
            printers: Vec::new(),
            selected_printer: String::new(),
            print_copies: 1,
            print_landscape: false,
            print_grayscale: false,
            print_paper_size: "A4".to_string(),
            print_fit_to_page: true,
            pending_action: None,
        }
    }

    fn activate(&mut self, ctx: &egui::Context) {
        if let Ok(raw) = crate::capture::capture_primary_screen_raw() {
            let width = raw.width;
            let height = raw.height;
            let color_image = crate::capture::raw_to_color_image(raw);
            
            self.screenshot = Some(Arc::new(image::DynamicImage::ImageRgba8(
                image::ImageBuffer::from_raw(width as u32, height as u32, color_image.as_raw().to_vec()).unwrap()
            )));

            self.texture = Some(ctx.load_texture("screenshot", color_image, Default::default()));
            
            self.selection = None;
            self.shapes.clear();
            self.active_shape = None;
            self.editing_text_index = None;
            self.pending_action = None;
            self.show_print_popup = false;
            self.is_active = true;
            
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
    }

    fn deactivate(&mut self, ctx: &egui::Context) {
        self.is_active = false;
        self.show_print_popup = false;
        self.pending_action = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(0.0, 0.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
    }

    fn request_final_screenshot(&mut self, ctx: &egui::Context, action: PendingAction) {
        self.pending_action = Some(action);
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
    }

    fn process_screenshot(&mut self, ctx: &egui::Context, color_image: egui::ColorImage) {
        let action = if let Some(a) = self.pending_action.take() { a } else { return };
        let sel_rect = if let Some(s) = self.selection { Rect::from_two_pos(s.min, s.max) } else { return };
        
        let ppp = ctx.pixels_per_point();
        let x = (sel_rect.min.x * ppp).round() as u32;
        let y = (sel_rect.min.y * ppp).round() as u32;
        let w = (sel_rect.width() * ppp).round() as u32;
        let h = (sel_rect.height() * ppp).round() as u32;

        let pixels = color_image.as_raw();
        let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(color_image.width() as u32, color_image.height() as u32, pixels.to_vec()).unwrap();
        let dynamic_img = image::DynamicImage::ImageRgba8(img_buffer);
        
        let crop_x = x.min(dynamic_img.width().saturating_sub(1));
        let crop_y = y.min(dynamic_img.height().saturating_sub(1));
        let crop_w = w.min(dynamic_img.width().saturating_sub(crop_x)).max(1);
        let crop_h = h.min(dynamic_img.height().saturating_sub(crop_y)).max(1);
        
        let cropped = dynamic_img.crop_imm(crop_x, crop_y, crop_w, crop_h);
        
        self.deactivate(ctx);

        thread::spawn(move || {
            match action {
                PendingAction::Copy => {
                    let _ = crate::actions::copy_to_clipboard(&cropped);
                }
                PendingAction::Save => {
                    let _ = crate::actions::save_to_file(&cropped);
                }
                PendingAction::Upload => {
                    if let Ok(url) = crate::actions::prntsc_upload(&cropped) {
                        let _ = webbrowser::open(&url);
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(url);
                        }
                    }
                }
                PendingAction::OCR => {
                    if let Ok(text) = crate::actions::image_to_text(&cropped) {
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(text);
                        }
                    }
                }
                PendingAction::Google => {
                    let _ = crate::actions::google_search(&cropped);
                }
                PendingAction::Print { printer, copies, landscape, grayscale, fit, paper } => {
                    let _ = crate::actions::print_image_to(&cropped, &printer, copies, landscape, grayscale, fit, &paper);
                }
            }
        });
    }

    fn prepare_print_preview(&mut self, ctx: &egui::Context, selection: Rect) {
        let full_img = Arc::clone(self.screenshot.as_ref().unwrap());
        let shapes = self.shapes.clone();
        let ppp = ctx.pixels_per_point();
        let processed = crate::render::render_and_crop(&full_img, &shapes, selection, ppp);
        let rgba = processed.to_rgba8();
        let color_image = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as _, rgba.height() as _], rgba.as_raw());
        self.cropped_preview = Some(ctx.load_texture("print_preview", color_image, Default::default()));
        self.printers = crate::actions::get_printers();
        if !self.printers.is_empty() { self.selected_printer = self.printers[0].clone(); }
        self.show_print_popup = true;
    }

    fn get_nodes(&self, rect: Rect) -> [Rect; 8] {
        let size = 8.0;
        [
            Rect::from_center_size(rect.left_top(), egui::vec2(size, size)),
            Rect::from_center_size(rect.right_top(), egui::vec2(size, size)),
            Rect::from_center_size(rect.right_bottom(), egui::vec2(size, size)),
            Rect::from_center_size(rect.left_bottom(), egui::vec2(size, size)),
            Rect::from_center_size(egui::pos2(rect.center().x, rect.top()), egui::vec2(size, size)),
            Rect::from_center_size(egui::pos2(rect.right(), rect.center().y), egui::vec2(size, size)),
            Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom()), egui::vec2(size, size)),
            Rect::from_center_size(egui::pos2(rect.left(), rect.center().y), egui::vec2(size, size)),
        ]
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Ok(_event) = MenuEvent::receiver().try_recv() {
            process::exit(0);
        }

        if self.trigger_flag.swap(false, Ordering::SeqCst) {
            self.activate(ctx);
        }

        let mut received_screenshot = None;
        ctx.input(|i| {
            for event in &i.raw.events {
                if let egui::Event::Screenshot { image, .. } = event {
                    received_screenshot = Some((**image).clone());
                }
            }
        });
        if let Some(img) = received_screenshot {
            self.process_screenshot(ctx, img);
            return;
        }

        if !self.is_active {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }

        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.deactivate(ctx);
            return;
        }

        let mut trigger_copy = false;
        let mut trigger_save = false;
        let mut trigger_undo = false;
        let mut trigger_upload = false;
        let mut trigger_google = false;
        let mut trigger_print = false;
        let mut trigger_select_all = false;

        ctx.input(|i| {
            for event in &i.events {
                match event {
                    egui::Event::Copy => trigger_copy = true,
                    egui::Event::Key { key: Key::C, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_copy = true,
                    egui::Event::Key { key: Key::S, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_save = true,
                    egui::Event::Key { key: Key::Z, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_undo = true,
                    egui::Event::Key { key: Key::D, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_upload = true,
                    egui::Event::Key { key: Key::G, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_google = true,
                    egui::Event::Key { key: Key::P, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_print = true,
                    egui::Event::Key { key: Key::A, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_select_all = true,
                    _ => {}
                }
            }
        });

        if trigger_undo { self.shapes.pop(); }
        if trigger_select_all {
            self.selection = Some(ctx.screen_rect());
        }
        
        let current_sel = self.selection.map(|s| Rect::from_two_pos(s.min, s.max));

        if let Some(sel) = current_sel {
            if trigger_copy { self.request_final_screenshot(ctx, PendingAction::Copy); return; }
            if trigger_save { self.request_final_screenshot(ctx, PendingAction::Save); return; }
            if trigger_upload { self.request_final_screenshot(ctx, PendingAction::Upload); return; }
            if trigger_google { self.request_final_screenshot(ctx, PendingAction::Google); return; }
            if trigger_print { self.prepare_print_preview(ctx, sel); return; }
        }

        if self.texture.is_none() {
            if let Some(img) = &self.screenshot {
                let rgba = img.as_rgba8().unwrap();
                let color_image = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as _, rgba.height() as _], rgba.as_raw());
                self.texture = Some(ctx.load_texture("screenshot", color_image, Default::default()));
            }
        }

        egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
            let screen_rect = ui.max_rect();
            if let Some(texture) = &self.texture { ui.painter().image(texture.id(), screen_rect, Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE); }

            let response = ui.interact(screen_rect, Id::new("main_interact"), egui::Sense::drag());
            let pointer_pos = ctx.pointer_interact_pos().unwrap_or(Pos2::ZERO).clamp(screen_rect.min, screen_rect.max);

            if let Some(sel) = current_sel {
                let dim = Color32::from_black_alpha(180);
                ui.painter().rect_filled(Rect::from_min_max(screen_rect.min, egui::pos2(screen_rect.max.x, sel.min.y)), 0.0, dim);
                ui.painter().rect_filled(Rect::from_min_max(egui::pos2(screen_rect.min.x, sel.max.y), screen_rect.max), 0.0, dim);
                ui.painter().rect_filled(Rect::from_min_max(egui::pos2(screen_rect.min.x, sel.min.y), egui::pos2(sel.min.x, sel.max.y)), 0.0, dim);
                ui.painter().rect_filled(Rect::from_min_max(egui::pos2(sel.max.x, sel.min.y), egui::pos2(screen_rect.max.x, sel.max.y)), 0.0, dim);
                
                if self.pending_action.is_none() {
                    ui.painter().rect_stroke(sel, 0.0, Stroke::new(1.0, Color32::WHITE));
                    let size_text = format!("{} x {}", sel.width().round(), sel.height().round());
                    ui.painter().text(sel.left_top() - egui::vec2(0.0, 20.0), egui::Align2::LEFT_TOP, size_text, egui::FontId::proportional(14.0), Color32::WHITE);
                    for (i, node) in self.get_nodes(sel).iter().enumerate() {
                        let node_color = if self.resizing_node == Some(i) { Color32::LIGHT_BLUE } else { Color32::WHITE };
                        ui.painter().rect_filled(*node, 0.0, node_color);
                    }
                }
            } else {
                ui.painter().rect_filled(screen_rect, 0.0, Color32::from_black_alpha(180)); 
                if self.pending_action.is_none() {
                    let text = "Select an area";
                    let font_id = egui::FontId::proportional(16.0);
                    let text_color = Color32::WHITE;
                    let offset = egui::vec2(15.0, 15.0);
                    ui.painter().text(pointer_pos + offset, egui::Align2::LEFT_TOP, text, font_id, text_color);
                }
            }

            for (idx, shape) in self.shapes.iter().enumerate() { 
                if Some(idx) != self.editing_text_index { self.render_shape(ui, shape); }
            }
            if let Some(shape) = &self.active_shape { self.render_shape(ui, shape); }

            if let Some(idx) = self.editing_text_index {
                if let Some(shape) = self.shapes.get_mut(idx) {
                    if let Some(pos) = shape.points.first() {
                        let text_color = shape.color;
                        egui::Area::new(Id::new(("edit_text", idx))).fixed_pos(*pos).show(ctx, |ui| {
                            let re = ui.add(egui::TextEdit::singleline(&mut shape.text).font(egui::FontId::proportional(20.0)).text_color(text_color).frame(false));
                            re.request_focus();
                            if re.lost_focus() || ctx.input(|i| i.key_pressed(Key::Enter)) { self.editing_text_index = None; }
                        });
                    }
                }
            }

            if let Some(sel) = current_sel {
                if !self.is_selecting && self.resizing_node.is_none() && !self.show_print_popup && self.pending_action.is_none() { self.show_toolbars(ctx, sel); }
            }

            if !self.show_print_popup && self.pending_action.is_none() && response.drag_started() && !ctx.is_pointer_over_area() {
                if let Some(sel) = current_sel {
                    let nodes = self.get_nodes(sel);
                    if let Some(idx) = nodes.iter().position(|n| n.contains(pointer_pos)) { self.resizing_node = Some(idx); }
                    else if sel.contains(pointer_pos) {
                        if self.current_tool == Tool::Text {
                            self.shapes.push(Shape { points: vec![pointer_pos], color: self.current_color, stroke_width: 2.0, tool: Tool::Text, text: String::new(), is_marker: false, opacity: 1.0 });
                            self.editing_text_index = Some(self.shapes.len() - 1);
                        } else {
                            self.active_shape = Some(Shape { points: vec![pointer_pos], color: self.current_color, stroke_width: if self.current_tool == Tool::Marker { 15.0 } else { 2.5 }, tool: self.current_tool, text: String::new(), is_marker: self.current_tool == Tool::Marker, opacity: if self.current_tool == Tool::Marker { self.marker_opacity } else { 1.0 } });
                        }
                    }
                    else { self.selection = Some(Rect::from_two_pos(pointer_pos, pointer_pos)); self.is_selecting = true; self.start_pos = Some(pointer_pos); }
                } else { self.is_selecting = true; self.start_pos = Some(pointer_pos); self.selection = Some(Rect::from_two_pos(pointer_pos, pointer_pos)); }
            }

            if !self.show_print_popup && self.pending_action.is_none() && response.dragged() {
                if self.is_selecting { if let Some(start) = self.start_pos { self.selection = Some(Rect::from_two_pos(start, pointer_pos)); } }
                else if let Some(idx) = self.resizing_node {
                    if let Some(mut sel) = self.selection {
                        match idx { 
                            0 => sel.min = pointer_pos, 1 => { sel.max.x = pointer_pos.x; sel.min.y = pointer_pos.y; }, 
                            2 => sel.max = pointer_pos, 3 => { sel.min.x = pointer_pos.x; sel.max.y = pointer_pos.y; },
                            4 => sel.min.y = pointer_pos.y, 5 => sel.max.x = pointer_pos.x, 
                            6 => sel.max.y = pointer_pos.y, 7 => sel.min.x = pointer_pos.x, _ => {} 
                        }
                        self.selection = Some(sel);
                    }
                } else if let Some(shape) = &mut self.active_shape { shape.points.push(pointer_pos); }
            }

            if response.drag_stopped() {
                self.is_selecting = false; self.resizing_node = None;
                if let Some(shape) = self.active_shape.take() { self.shapes.push(shape); }
                if let Some(sel) = self.selection {
                    self.selection = Some(Rect::from_two_pos(sel.min, sel.max));
                }
            }

            if self.show_print_popup { self.show_print_window(ctx); }
        });
        ctx.request_repaint();
    }
}

impl OverlayApp {
    fn show_print_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Lightshot - Print").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.vertical_centered(|ui| {
                ui.heading("Print Selection");
                ui.add_space(10.0);
                if let Some(texture) = &self.cropped_preview {
                    let max_size = egui::vec2(380.0, 200.0);
                    let tex_size = texture.size_vec2();
                    let factor = (max_size.x / tex_size.x).min(max_size.y / tex_size.y).min(1.0);
                    ui.add(egui::Image::new(texture).max_size(tex_size * factor));
                }
                ui.add_space(15.0);
                egui::Grid::new("print_grid").num_columns(2).spacing([10.0, 10.0]).show(ui, |ui| {
                    ui.label("Printer:");
                    egui::ComboBox::from_id_source("printer_select").selected_text(&self.selected_printer).width(250.0).show_ui(ui, |ui| {
                        for printer in &self.printers { ui.selectable_value(&mut self.selected_printer, printer.clone(), printer); }
                    });
                    ui.end_row();
                    ui.label("Orientation:");
                    ui.horizontal(|ui| { ui.radio_value(&mut self.print_landscape, false, "Portrait"); ui.radio_value(&mut self.print_landscape, true, "Landscape"); });
                    ui.end_row();
                    ui.label("Color Mode:");
                    ui.horizontal(|ui| { ui.radio_value(&mut self.print_grayscale, false, "Color"); ui.radio_value(&mut self.print_grayscale, true, "Grayscale"); });
                    ui.end_row();
                    ui.label("Copies:");
                    ui.add(egui::DragValue::new(&mut self.print_copies).clamp_range(1..=99));
                    ui.end_row();
                    ui.label("Paper Size:");
                    egui::ComboBox::from_id_source("paper_size").selected_text(&self.print_paper_size).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.print_paper_size, "A4".to_string(), "A4");
                        ui.selectable_value(&mut self.print_paper_size, "Letter".to_string(), "Letter");
                        ui.selectable_value(&mut self.print_paper_size, "Legal".to_string(), "Legal");
                    });
                    ui.end_row();
                    ui.label("Scaling:");
                    ui.horizontal(|ui| { ui.radio_value(&mut self.print_fit_to_page, true, "Fit to Page"); ui.radio_value(&mut self.print_fit_to_page, false, "Actual Size"); });
                    ui.end_row();
                });
                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    let btn_width = (ui.available_width() - 10.0) / 2.0;
                    if ui.add_sized([btn_width, 30.0], egui::Button::new("Print")).clicked() {
                        let printer_name = self.selected_printer.clone();
                        let copies = self.print_copies;
                        let landscape = self.print_landscape;
                        let grayscale = self.print_grayscale;
                        let fit = self.print_fit_to_page;
                        let paper = self.print_paper_size.clone();
                        
                        self.request_final_screenshot(ctx, PendingAction::Print {
                            printer: printer_name,
                            copies,
                            landscape,
                            grayscale,
                            fit,
                            paper,
                        });
                    }
                    if ui.add_sized([btn_width, 30.0], egui::Button::new("Cancel")).clicked() { self.show_print_popup = false; self.cropped_preview = None; }
                });
            });
        });
    }

    fn render_shape(&self, ui: &mut egui::Ui, shape: &Shape) {
        let painter = ui.painter();
        let color = if shape.is_marker { shape.color.gamma_multiply(shape.opacity) } else { shape.color };
        let stroke = Stroke::new(shape.stroke_width, color);
        match shape.tool {
            Tool::Pen | Tool::Marker => {
                if shape.points.len() > 1 {
                    let mut path_points = shape.points.clone();
                    if path_points.len() > 2 {
                        for i in (1..path_points.len() - 1).rev() {
                            let p1 = path_points[i];
                            let p2 = path_points[i+1];
                            path_points[i] = egui::pos2((p1.x + p2.x) / 2.0, (p1.y + p2.y) / 2.0);
                        }
                    }
                    painter.add(egui::epaint::Shape::Path(egui::epaint::PathShape { points: path_points, closed: false, fill: Color32::TRANSPARENT, stroke }));
                }
            }
            Tool::Line => { if shape.points.len() > 1 { painter.line_segment([shape.points[0], *shape.points.last().unwrap()], stroke); } }
            Tool::Rect => { if shape.points.len() > 1 { painter.rect_stroke(Rect::from_two_pos(shape.points[0], *shape.points.last().unwrap()), 0.0, stroke); } }
            Tool::Arrow => {
                if shape.points.len() > 1 {
                    let start = shape.points[0];
                    let end = *shape.points.last().unwrap();
                    painter.arrow(start, end - start, stroke);
                }
            }
            Tool::Text => { if let Some(pos) = shape.points.first() { painter.text(*pos, egui::Align2::LEFT_TOP, &shape.text, egui::FontId::proportional(20.0), color); } }
        }
    }

    fn show_toolbars(&mut self, ctx: &egui::Context, selection: Rect) {
        let toolbar_color = Color32::from_rgb(30, 30, 30);
        let spacing = 4.0;
        let v_height = 240.0;
        use egui_nerdfonts::regular::*;
        egui::Window::new("tools")
            .fixed_pos(egui::pos2(selection.max.x + spacing, selection.max.y - v_height + 50.0))
            .title_bar(false).resizable(false).collapsible(false).fixed_size([25.0, v_height])
            .frame(egui::Frame::window(&ctx.style()).fill(toolbar_color).stroke(Stroke::new(1.0, Color32::GRAY)).inner_margin(2.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if ui.selectable_label(self.current_tool == Tool::Pen, PENCIL).on_hover_text("Pen").clicked() { self.current_tool = Tool::Pen; }
                    ui.add_space(2.0);
                    if ui.selectable_label(self.current_tool == Tool::Line, SLASH).on_hover_text("Line").clicked() { self.current_tool = Tool::Line; }
                    ui.add_space(2.0);
                    if ui.selectable_label(self.current_tool == Tool::Arrow, ARROW_RIGHT).on_hover_text("Arrow").clicked() { self.current_tool = Tool::Arrow; }
                    ui.add_space(2.0);
                    if ui.selectable_label(self.current_tool == Tool::Rect, SQUARE).on_hover_text("Rectangle").clicked() { self.current_tool = Tool::Rect; }
                    ui.add_space(2.0);
                    if ui.selectable_label(self.current_tool == Tool::Marker, MARKER).on_hover_text("Marker").clicked() { self.current_tool = Tool::Marker; }
                    ui.add_space(2.0);
                    if ui.selectable_label(self.current_tool == Tool::Text, FONT).on_hover_text("Text Tool").clicked() { self.current_tool = Tool::Text; }
                    ui.separator();
                    ui.scope(|ui| { ui.spacing_mut().interact_size = egui::vec2(20.0, 20.0); ui.color_edit_button_srgba(&mut self.current_color) }).response.on_hover_text("Change Color");
                    ui.add_space(2.0);
                    if ui.button(UNDO).on_hover_text("Undo (Ctrl+Z)").clicked() { self.shapes.pop(); }
                });
            });

        let h_width = 280.0;
        egui::Window::new("actions")
            .fixed_pos(egui::pos2(selection.max.x - h_width + 50.0, selection.max.y + spacing))
            .title_bar(false).resizable(false).collapsible(false).fixed_size([h_width, 35.0])
            .frame(egui::Frame::window(&ctx.style()).fill(toolbar_color).stroke(Stroke::new(1.0, Color32::GRAY)).inner_margin(4.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button(CLOUD).on_hover_text("Cloud Upload (Ctrl+D)").clicked() { self.request_final_screenshot(ctx, PendingAction::Upload); }
                    if ui.button(GOOGLE).on_hover_text("Google Image Search (Ctrl+G)").clicked() { self.request_final_screenshot(ctx, PendingAction::Google); }
                    if ui.button(ALIGN_LEFT).on_hover_text("Image to Text (OCR)").clicked() { self.request_final_screenshot(ctx, PendingAction::OCR); }
                    if ui.button(PRINT).on_hover_text("Print Selection (Ctrl+P)").clicked() { self.prepare_print_preview(ctx, selection); }
                    if ui.button(SAVE).on_hover_text("Save (Ctrl+S)").clicked() { self.request_final_screenshot(ctx, PendingAction::Save); }
                    if ui.button(COPY).on_hover_text("Copy (Ctrl+C)").clicked() { self.request_final_screenshot(ctx, PendingAction::Copy); }
                    if ui.button(CLOSE).on_hover_text("Close (Esc)").clicked() { self.deactivate(ctx); }
                });
            });

        if self.current_tool == Tool::Marker {
            let marker_bar_pos = egui::pos2(selection.max.x + spacing + 31.0, selection.max.y - 148.0 + 50.0);
            egui::Window::new("marker_settings").fixed_pos(marker_bar_pos).title_bar(false).resizable(false).collapsible(false).frame(egui::Frame::window(&ctx.style()).fill(toolbar_color).stroke(Stroke::new(1.0, Color32::GRAY)).inner_margin(4.0)).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_at_least(egui::vec2(14.0, 14.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 7.0, self.current_color.gamma_multiply(self.marker_opacity));
                    ui.painter().circle_stroke(rect.center(), 7.0, Stroke::new(1.0, Color32::GRAY));
                    ui.add(egui::Slider::new(&mut self.marker_opacity, 0.1..=1.0).show_value(false).trailing_fill(true));
                });
            });
        }
    }
}
