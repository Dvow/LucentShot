use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Id, Key};
use image::DynamicImage;
use std::sync::Arc;
use std::thread;
use std::process;
use std::sync::atomic::{AtomicBool, Ordering};
use tray_icon::menu::MenuEvent;
use tray_icon::{TrayIconEvent, MouseButton};
use crate::config::{PendingAction, Shape, Tool};

pub const MENU_ID_QUIT: &str = "menu_quit";
pub const MENU_ID_SETTINGS: &str = "menu_settings";

pub struct OverlayApp {
    pub screenshot: Option<Arc<DynamicImage>>,
    pub texture: Option<egui::TextureHandle>,
    pub selection: Option<Rect>,
    pub is_selecting: bool,
    pub resizing_node: Option<usize>,
    pub start_pos: Option<Pos2>,
    pub current_tool: Tool,
    pub current_color: Color32,
    pub last_saved_color: Color32,
    pub shapes: Vec<Shape>,
    pub redo_stack: Vec<Shape>,
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
    pub show_settings: bool,
    pub settings_state: crate::ui::SettingsWindowState,
    pub config: crate::config::ConfigImpl,
    pub hotkey_handle: crate::hotkey::HotkeyHandle,
    pub hotkey_rx: std::sync::mpsc::Receiver<crate::hotkey::HotkeyEvent>,
    pub pending_action: Option<PendingAction>,
    ocr_clipboard_tx: std::sync::mpsc::Sender<String>,
    ocr_clipboard_rx: std::sync::mpsc::Receiver<String>,
}

impl OverlayApp {
    fn handle_instant_action(&self, action: PendingAction) {
        let include_cursor = self.config.general_capture_cursor;
        let auto_copy_link = self.config.general_auto_copy_link;
        let auto_close_upload = self.config.general_auto_close_upload;
        let show_notifications = self.config.general_show_notifications;
        thread::spawn(move || {
            let Ok(raw) = crate::capture::capture_primary_screen_raw(include_cursor) else { return };
            let width = raw.width;
            let height = raw.height;
            let color_image = crate::capture::raw_to_color_image(raw);
            let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                width as u32,
                height as u32,
                color_image.as_raw().to_vec(),
            )
            .unwrap();
            let img = image::DynamicImage::ImageRgba8(img_buffer);
            match action {
                PendingAction::Save => {
                    if let Ok(true) = crate::actions::save_to_file(&img) {
                        if show_notifications {
                            crate::notification::show("Save", "Image saved");
                        }
                    }
                }
                PendingAction::Upload => {
                    if let Ok(url) = crate::actions::prntsc_upload(&img) {
                        handle_upload_result(&url, auto_copy_link, auto_close_upload);
                        if show_notifications {
                            crate::notification::show("Upload", "Image uploaded");
                        }
                    }
                }
                _ => {}
            }
        });
    }

    fn handle_copy_focused_window(&self) {
        let show_notifications = self.config.general_show_notifications;
        thread::spawn(move || {
            let Ok(raw) = crate::capture::capture_focused_window_raw() else { return };
            let color_image = crate::capture::raw_to_color_image(raw);
            let img_buffer = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                color_image.width() as u32,
                color_image.height() as u32,
                color_image.as_raw().to_vec(),
            )
            .unwrap();
            let img = image::DynamicImage::ImageRgba8(img_buffer);
            if crate::actions::copy_to_clipboard(&img).is_ok() && show_notifications {
                crate::notification::show("Copy", "Copied to clipboard");
            }
        });
    }

    pub fn new_background(
        trigger_flag: Arc<AtomicBool>,
        hotkey_handle: crate::hotkey::HotkeyHandle,
        hotkey_rx: std::sync::mpsc::Receiver<crate::hotkey::HotkeyEvent>,
    ) -> Self {
        let (ocr_clipboard_tx, ocr_clipboard_rx) = std::sync::mpsc::channel();
        let config = crate::config::cfg().clone();
        let saved_color = egui::Color32::from_rgba_unmultiplied(
            config.color_r,
            config.color_g,
            config.color_b,
            config.color_a,
        );
        Self {
            screenshot: None,
            texture: None,
            selection: None,
            is_selecting: false,
            resizing_node: None,
            start_pos: None,
            current_tool: Tool::Pen,
            current_color: saved_color,
            last_saved_color: saved_color,
            shapes: Vec::new(),
            redo_stack: Vec::new(),
            active_shape: None,
            is_active: false,
            trigger_flag,
            marker_opacity: config.marker_opacity,
            editing_text_index: None,
            show_print_popup: false,
            cropped_preview: None,
            printers: Vec::new(),
            selected_printer: config.print_selected_printer.clone(),
            print_copies: config.print_copies,
            print_landscape: config.print_landscape,
            print_grayscale: config.print_grayscale,
            print_paper_size: config.print_paper.clone(),
            print_fit_to_page: config.print_fit,
            show_settings: false,
            settings_state: crate::ui::SettingsWindowState::default(),
            config,
            hotkey_handle,
            hotkey_rx,
            pending_action: None,
            ocr_clipboard_tx,
            ocr_clipboard_rx,
        }
    }

    fn activate(&mut self, ctx: &egui::Context) {
        if self.show_settings {
            self.close_settings(ctx);
        }
        if let Ok(raw) = crate::capture::capture_primary_screen_raw(self.config.general_capture_cursor) {
            let width = raw.width;
            let height = raw.height;
            let color_image = crate::capture::raw_to_color_image(raw);
            
            self.screenshot = Some(Arc::new(image::DynamicImage::ImageRgba8(
                image::ImageBuffer::from_raw(width as u32, height as u32, color_image.as_raw().to_vec()).unwrap()
            )));

            self.texture = Some(ctx.load_texture("screenshot", color_image, Default::default()));
            
            if !self.config.general_keep_selected_area {
                self.selection = None;
            }
            self.shapes.clear();
            self.redo_stack.clear();
            self.active_shape = None;
            self.editing_text_index = None;
            self.pending_action = None;
            self.current_tool = Tool::Pen;
            self.show_settings = false;
            self.is_active = true;
            
            ctx.memory_mut(|m| m.close_popup());
            let (vx, vy, vw, vh) = crate::capture::get_virtual_screen_bounds();
            let ppp = ctx.pixels_per_point();
            let pos = egui::pos2(vx as f32 / ppp, vy as f32 / ppp);
            let size = egui::vec2(vw as f32 / ppp, vh as f32 / ppp);
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
            self.hotkey_handle.set_listening(true);
        }
    }

    fn deactivate(&mut self, ctx: &egui::Context) {
        self.is_active = false;
        self.hotkey_handle.set_listening(false);
        self.show_print_popup = false;
        self.pending_action = None;
        self.texture = None;
        self.screenshot = None;
        self.cropped_preview = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1.0, 1.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(-10000.0, -10000.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Lightshot Clone".to_string()));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    }

    fn open_settings(&mut self, ctx: &egui::Context) {
        self.show_settings = true;
        self.hotkey_handle.set_listening(true);
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(520.0, 360.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(120.0, 120.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Settings".to_string()));
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn close_settings(&mut self, ctx: &egui::Context) {
        self.show_settings = false;
        if !self.is_active {
            self.hotkey_handle.set_listening(false);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1.0, 1.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Transparent(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("Lightshot Clone".to_string()));
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(-10000.0, -10000.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }
        ctx.request_repaint();
    }

    fn execute_action_immediate(&mut self, ctx: &egui::Context, action: PendingAction) {
        let Some(screenshot) = self.screenshot.as_ref().map(Arc::clone) else { return };
        let Some(sel) = self.selection else { return };
        let shapes = self.shapes.clone();
        let ppp = ctx.pixels_per_point();

        self.deactivate(ctx);

        let cropped = crate::render::render_and_crop(&screenshot, &shapes, sel, ppp);
        let auto_copy_link = self.config.general_auto_copy_link;
        let auto_close_upload = self.config.general_auto_close_upload;
        let show_notifications = self.config.general_show_notifications;
        let ocr_tx = self.ocr_clipboard_tx.clone();
        thread::spawn(move || {
            match action {
                PendingAction::Copy => {
                    if crate::actions::copy_to_clipboard(&cropped).is_ok() && show_notifications {
                        crate::notification::show("Copy", "Copied to clipboard");
                    }
                }
                PendingAction::Save => {
                    if let Ok(true) = crate::actions::save_to_file(&cropped) {
                        if show_notifications {
                            crate::notification::show("Save", "Image saved");
                        }
                    }
                }
                PendingAction::Upload => {
                    if let Ok(url) = crate::actions::prntsc_upload(&cropped) {
                        handle_upload_result(&url, auto_copy_link, auto_close_upload);
                        if show_notifications {
                            crate::notification::show("Upload", "Image uploaded");
                        }
                    }
                }
                PendingAction::Ocr => {
                    match crate::actions::image_to_text(&cropped) {
                        Ok(text) => { let _ = ocr_tx.send(text); }
                        Err(e) => crate::actions::show_ocr_error(&e.to_string()),
                    }
                }
                PendingAction::Speak => {
                    if let Err(e) = crate::actions::image_to_speech(&cropped) {
                        crate::actions::show_ocr_error(&format!("Image to Speech failed: {e}"));
                    }
                }
                PendingAction::Google => {
                    if let Err(e) = crate::actions::google_search(&cropped) {
                        eprintln!("Google search failed: {e}");
                    }
                }
                PendingAction::Print { printer, copies, landscape, grayscale, paper } => {
                    if let Err(e) = crate::actions::print_image_to(&cropped, &printer, copies, landscape, grayscale, &paper) {
                        eprintln!("Print failed: {e}");
                    }
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
        if self.selected_printer.is_empty()
            || !self.printers.iter().any(|p| p == &self.selected_printer)
        {
            if let Some(first) = self.printers.first() {
                self.selected_printer = first.clone();
            }
        }
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

fn handle_upload_result(url: &str, auto_copy_link: bool, auto_close_upload: bool) {
    if !auto_close_upload {
        let _ = crate::actions::open_url(url);
    }
    if auto_copy_link {
        let _ = crate::actions::set_clipboard_text(url);
    }
}

impl eframe::App for OverlayApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(text) = self.ocr_clipboard_rx.try_recv() {
            let _ = crate::actions::set_clipboard_text(&text);
            if self.config.general_show_notifications {
                crate::notification::show("OCR", "Text copied to clipboard");
            }
        }

        if let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id == MENU_ID_QUIT {
                process::exit(0);
            }
            if event.id == MENU_ID_SETTINGS {
                self.open_settings(ctx);
            }
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, .. } = event {
                self.trigger_flag.store(true, Ordering::SeqCst);
                break;
            }
        }

        while let Ok(event) = self.hotkey_rx.try_recv() {
            match event {
                crate::hotkey::HotkeyEvent::InstantSave => {
                    self.handle_instant_action(PendingAction::Save);
                }
                crate::hotkey::HotkeyEvent::InstantUpload => {
                    self.handle_instant_action(PendingAction::Upload);
                }
                crate::hotkey::HotkeyEvent::CopyFocusedWindow => {
                    self.handle_copy_focused_window();
                }
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.show_settings {
                self.close_settings(ctx);
            } else if self.is_active {
                self.deactivate(ctx);
            }
            return;
        }

        if self.trigger_flag.swap(false, Ordering::SeqCst) && !self.is_active {
            self.activate(ctx);
        }

        if !self.is_active && !self.show_settings {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
            return;
        }

        if self.current_color != self.last_saved_color {
            {
                let mut config = crate::config::cfg_mut();
                config.color_r = self.current_color.r();
                config.color_g = self.current_color.g();
                config.color_b = self.current_color.b();
                config.color_a = self.current_color.a();
            }
            crate::config::save();
            self.last_saved_color = self.current_color;
        }

        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.deactivate(ctx);
            return;
        }

        let mut trigger_copy = false;
        let mut trigger_save = false;
        let mut trigger_undo = false;
        let mut trigger_redo = false;
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
                    egui::Event::Key { key: Key::Z, pressed: true, modifiers, .. } if (modifiers.ctrl || modifiers.command) && !modifiers.shift => trigger_undo = true,
                    egui::Event::Key { key: Key::Z, pressed: true, modifiers, .. } if (modifiers.ctrl || modifiers.command) && modifiers.shift => trigger_redo = true,
                    egui::Event::Key { key: Key::D, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_upload = true,
                    egui::Event::Key { key: Key::G, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_google = true,
                    egui::Event::Key { key: Key::P, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_print = true,
                    egui::Event::Key { key: Key::A, pressed: true, modifiers, .. } if modifiers.ctrl || modifiers.command => trigger_select_all = true,
                    _ => {}
                }
            }
        });

        if trigger_undo {
            if let Some(shape) = self.shapes.pop() {
                self.redo_stack.push(shape);
            }
        }
        if trigger_redo {
            if let Some(shape) = self.redo_stack.pop() {
                self.shapes.push(shape);
            }
        }
        if trigger_select_all {
            self.selection = Some(ctx.screen_rect());
        }
        
        let current_sel = self.selection.map(|s| Rect::from_two_pos(s.min, s.max));

        if let Some(sel) = current_sel {
            if trigger_copy { self.execute_action_immediate(ctx, PendingAction::Copy); return; }
            if trigger_save { self.execute_action_immediate(ctx, PendingAction::Save); return; }
            if trigger_upload { self.execute_action_immediate(ctx, PendingAction::Upload); return; }
            if trigger_google { self.execute_action_immediate(ctx, PendingAction::Google); return; }
            if trigger_print { self.prepare_print_preview(ctx, sel); return; }
        }

        egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| {
            let screen_rect = ui.max_rect();
            if self.is_active {
                if let Some(texture) = &self.texture {
                    ui.painter().image(
                        texture.id(),
                        screen_rect,
                        Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }

                let response = ui.interact(screen_rect, Id::new("main_interact"), egui::Sense::drag());
                let pointer_pos = ctx.pointer_interact_pos().unwrap_or(Pos2::ZERO).clamp(screen_rect.min, screen_rect.max);

                if let Some(sel) = current_sel {
                    let dim = Color32::from_black_alpha(180);
                    ui.painter().rect_filled(Rect::from_min_max(screen_rect.min, egui::pos2(screen_rect.max.x, sel.min.y)), 0.0, dim);
                    ui.painter().rect_filled(Rect::from_min_max(egui::pos2(screen_rect.min.x, sel.max.y), screen_rect.max), 0.0, dim);
                    ui.painter().rect_filled(Rect::from_min_max(egui::pos2(screen_rect.min.x, sel.min.y), egui::pos2(sel.min.x, sel.max.y)), 0.0, dim);
                    ui.painter().rect_filled(Rect::from_min_max(egui::pos2(sel.max.x, sel.min.y), egui::pos2(screen_rect.max.x, sel.max.y)), 0.0, dim);
                    
                    ui.painter().rect_stroke(sel, 0.0, Stroke::new(1.0, Color32::WHITE));
                    let size_text = format!("{} x {}", sel.width().round(), sel.height().round());
                    ui.painter().text(sel.left_top() - egui::vec2(0.0, 20.0), egui::Align2::LEFT_TOP, size_text, egui::FontId::proportional(14.0), Color32::WHITE);
                    let nodes = self.get_nodes(sel);
                    let hovered_node = nodes.iter().position(|n| n.contains(pointer_pos));
                    for (i, node) in nodes.iter().enumerate() {
                        let node_color = if self.resizing_node == Some(i) {
                            Color32::LIGHT_BLUE
                        } else if hovered_node == Some(i) {
                            Color32::from_rgb(180, 220, 255)
                        } else {
                            Color32::WHITE
                        };
                        ui.painter().rect_filled(*node, 0.0, node_color);
                    }
                } else {
                    ui.painter().rect_filled(screen_rect, 0.0, Color32::from_black_alpha(180)); 
                    let text = "Select an area";
                    let font_id = egui::FontId::proportional(16.0);
                    let text_color = Color32::WHITE;
                    let offset = egui::vec2(15.0, 15.0);
                    ui.painter().text(pointer_pos + offset, egui::Align2::LEFT_TOP, text, font_id, text_color);
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
                    let ppp = ctx.pixels_per_point();
                    let min_pts = 5.0 / ppp;
                    let has_meaningful_selection = sel.width() >= min_pts && sel.height() >= min_pts;
                    if has_meaningful_selection && !self.is_selecting && self.resizing_node.is_none() && !self.show_print_popup {
                        self.show_toolbars(ctx, sel);
                    }
                }

                if !self.show_print_popup
                    && response.drag_started()
                    && !ctx.is_pointer_over_area()
                {
                    if let Some(sel) = current_sel {
                        let nodes = self.get_nodes(sel);
                        if let Some(idx) = nodes.iter().position(|n| n.contains(pointer_pos)) { self.resizing_node = Some(idx); }
                        else if sel.contains(pointer_pos) {
                            if self.current_tool == Tool::Text {
                                self.redo_stack.clear();
                                self.shapes.push(Shape { points: vec![pointer_pos], color: self.current_color, stroke_width: 2.0, tool: Tool::Text, text: String::new(), is_marker: false, opacity: 1.0 });
                                self.editing_text_index = Some(self.shapes.len() - 1);
                            } else {
                                self.active_shape = Some(Shape { points: vec![pointer_pos], color: self.current_color, stroke_width: if self.current_tool == Tool::Marker { 15.0 } else { 2.5 }, tool: self.current_tool, text: String::new(), is_marker: self.current_tool == Tool::Marker, opacity: if self.current_tool == Tool::Marker { self.marker_opacity } else { 1.0 } });
                            }
                        }
                        else { self.selection = Some(Rect::from_two_pos(pointer_pos, pointer_pos)); self.is_selecting = true; self.start_pos = Some(pointer_pos); }
                    } else { self.is_selecting = true; self.start_pos = Some(pointer_pos); self.selection = Some(Rect::from_two_pos(pointer_pos, pointer_pos)); }
                }

                if !self.show_print_popup && response.dragged() {
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
                    } else if let Some(shape) = &mut self.active_shape {
                        let min_dist = if shape.is_marker {
                            (shape.stroke_width * 0.35).max(2.0)
                        } else {
                            1.0
                        };
                        let should_push = match shape.points.last() {
                            Some(last) => pointer_pos.distance(*last) >= min_dist,
                            None => true,
                        };
                        if should_push {
                            shape.points.push(pointer_pos);
                        }
                    }
                }

                if response.drag_stopped() {
                    self.is_selecting = false; self.resizing_node = None;
                    if let Some(shape) = self.active_shape.take() { self.redo_stack.clear(); self.shapes.push(shape); }
                    if let Some(sel) = self.selection {
                        self.selection = Some(Rect::from_two_pos(sel.min, sel.max));
                    }
                }

                if self.show_print_popup { self.show_print_window(ctx); }
            }

            if self.show_settings { self.show_settings_window(ctx); }
        });
        ctx.request_repaint();
    }
}

impl OverlayApp {
    fn show_print_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Lightshot - Print").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).show(ctx, |ui| {
            ui.set_width(400.0);
            ui.vertical_centered(|ui| {
                let before = (
                    self.selected_printer.clone(),
                    self.print_copies,
                    self.print_landscape,
                    self.print_grayscale,
                    self.print_fit_to_page,
                    self.print_paper_size.clone(),
                );
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
                        self.execute_action_immediate(ctx, PendingAction::Print {
                            printer: self.selected_printer.clone(),
                            copies: self.print_copies,
                            landscape: self.print_landscape,
                            grayscale: self.print_grayscale,
                            paper: self.print_paper_size.clone(),
                        });
                    }
                    if ui.add_sized([btn_width, 30.0], egui::Button::new("Cancel")).clicked() { self.show_print_popup = false; self.cropped_preview = None; }
                });
                let after = (
                    self.selected_printer.clone(),
                    self.print_copies,
                    self.print_landscape,
                    self.print_grayscale,
                    self.print_fit_to_page,
                    self.print_paper_size.clone(),
                );
                if before != after {
                    {
                        let mut config = crate::config::cfg_mut();
                        config.print_selected_printer = self.selected_printer.clone();
                        config.print_copies = self.print_copies;
                        config.print_landscape = self.print_landscape;
                        config.print_grayscale = self.print_grayscale;
                        config.print_fit = self.print_fit_to_page;
                        config.print_paper = self.print_paper_size.clone();
                    }
                    crate::config::save();
                }
            });
        });
    }

    fn show_settings_window(&mut self, ctx: &egui::Context) {
        let before_config = self.config.clone();
        crate::ui::show_settings_window(
            ctx,
            &mut self.settings_state,
            &mut self.config,
        );
        if before_config != self.config {
            {
                let mut config = crate::config::cfg_mut();
                *config = self.config.clone();
            }
            crate::config::save();
        }

        let before_hotkey = (
            before_config.hotkey_general_enabled,
            before_config.hotkey_general_key,
            before_config.hotkey_general_ctrl,
            before_config.hotkey_general_shift,
            before_config.hotkey_general_alt,
            before_config.hotkey_general_win,
            before_config.hotkey_instant_save_fullscreen,
            before_config.hotkey_instant_upload_fullscreen,
            before_config.hotkey_copy_focused_window,
            before_config.hotkey_instant_save_combo.clone(),
            before_config.hotkey_instant_upload_combo.clone(),
            before_config.hotkey_copy_focused_window_combo.clone(),
        );
        let after_hotkey = (
            self.config.hotkey_general_enabled,
            self.config.hotkey_general_key,
            self.config.hotkey_general_ctrl,
            self.config.hotkey_general_shift,
            self.config.hotkey_general_alt,
            self.config.hotkey_general_win,
            self.config.hotkey_instant_save_fullscreen,
            self.config.hotkey_instant_upload_fullscreen,
            self.config.hotkey_copy_focused_window,
            self.config.hotkey_instant_save_combo.clone(),
            self.config.hotkey_instant_upload_combo.clone(),
            self.config.hotkey_copy_focused_window_combo.clone(),
        );
        if before_hotkey != after_hotkey {
            self.hotkey_handle.update(crate::config::hotkey_config(&self.config));
        }
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
                    let dir = end - start;
                    let len = dir.length();
                    if len > 0.0 {
                        let unit = dir / len;
                        let head_len = 14.4;
                        let head_wid = 8.4;
                        let tip = end;
                        let left = tip - unit * head_len + egui::vec2(-unit.y, unit.x) * (head_wid * 0.5);
                        let right = tip - unit * head_len + egui::vec2(unit.y, -unit.x) * (head_wid * 0.5);
                        let shaft_end = tip - unit * head_len;
                        let fill_color = if shape.is_marker { shape.color.gamma_multiply(shape.opacity) } else { shape.color };
                        painter.line_segment([start, shaft_end], stroke);
                        painter.add(egui::epaint::Shape::convex_polygon(
                            vec![tip, left, right],
                            fill_color,
                            Stroke::NONE,
                        ));
                        painter.line_segment([tip, left], stroke);
                        painter.line_segment([tip, right], stroke);
                    }
                }
            }
            Tool::Text => { if let Some(pos) = shape.points.first() { painter.text(*pos, egui::Align2::LEFT_TOP, &shape.text, egui::FontId::proportional(20.0), color); } }
        }
    }

    fn show_toolbars(&mut self, ctx: &egui::Context, selection: Rect) {
        const BTN_SIZE: f32 = 18.0;
        let icon_color = Color32::WHITE;
        let toolbar_color = Color32::from_rgb(30, 30, 30);
        let spacing = 4.0;
        let v_height = 240.0;
        use egui_nerdfonts::regular::*;
        egui::Window::new("tools")
            .fixed_pos(egui::pos2(selection.max.x + spacing, selection.max.y - v_height + 50.0))
            .title_bar(false).resizable(false).collapsible(false).fixed_size([BTN_SIZE + 4.0, v_height])
            .frame(egui::Frame::window(&ctx.style()).fill(toolbar_color).stroke(Stroke::new(1.0, Color32::GRAY)).inner_margin(2.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::SelectableLabel::new(self.current_tool == Tool::Pen, egui::RichText::new(PENCIL).color(icon_color))).on_hover_text("Pen").clicked() { self.current_tool = Tool::Pen; }
                    ui.add_space(2.0);
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::SelectableLabel::new(self.current_tool == Tool::Line, egui::RichText::new(SLASH).color(icon_color))).on_hover_text("Line").clicked() { self.current_tool = Tool::Line; }
                    ui.add_space(2.0);
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::SelectableLabel::new(self.current_tool == Tool::Arrow, egui::RichText::new(ARROW_RIGHT).color(icon_color))).on_hover_text("Arrow").clicked() { self.current_tool = Tool::Arrow; }
                    ui.add_space(2.0);
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::SelectableLabel::new(self.current_tool == Tool::Rect, egui::RichText::new(SQUARE).color(icon_color))).on_hover_text("Rectangle").clicked() { self.current_tool = Tool::Rect; }
                    ui.add_space(2.0);
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::SelectableLabel::new(self.current_tool == Tool::Marker, egui::RichText::new(MARKER).color(icon_color))).on_hover_text("Marker").clicked() { self.current_tool = Tool::Marker; }
                    ui.add_space(2.0);
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::SelectableLabel::new(self.current_tool == Tool::Text, egui::RichText::new(FONT).color(icon_color))).on_hover_text("Text Tool").clicked() { self.current_tool = Tool::Text; }
                    ui.separator();
                    let color_response = ui
                        .scope(|ui| {
                            ui.spacing_mut().interact_size = egui::vec2(BTN_SIZE, BTN_SIZE);
                            ui.color_edit_button_srgba(&mut self.current_color)
                        })
                        .response
                        .on_hover_text("Change Color");
                    if color_response.changed() {
                        self.config.color_r = self.current_color.r();
                        self.config.color_g = self.current_color.g();
                        self.config.color_b = self.current_color.b();
                        self.config.color_a = self.current_color.a();
                        {
                            let mut config = crate::config::cfg_mut();
                            *config = self.config.clone();
                        }
                        crate::config::save();
                    }
                    ui.add_space(2.0);
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(UNDO).color(icon_color))).on_hover_text("Undo (Ctrl+Z)").clicked() {
                        if let Some(shape) = self.shapes.pop() { self.redo_stack.push(shape); }
                    }
                });
            });

        let h_width = 280.0;
        egui::Window::new("actions")
            .fixed_pos(egui::pos2(selection.max.x - h_width + 50.0, selection.max.y + spacing))
            .title_bar(false).resizable(false).collapsible(false).fixed_size([h_width, BTN_SIZE + 8.0])
            .frame(egui::Frame::window(&ctx.style()).fill(toolbar_color).stroke(Stroke::new(1.0, Color32::GRAY)).inner_margin(4.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(CLOUD).color(icon_color))).on_hover_text("Cloud Upload (Ctrl+D)").clicked() { self.execute_action_immediate(ctx, PendingAction::Upload); }
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(GOOGLE).color(icon_color))).on_hover_text("Google Image Search (Ctrl+G)").clicked() { self.execute_action_immediate(ctx, PendingAction::Google); }
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(ALIGN_LEFT).color(icon_color))).on_hover_text("Image to Text (OCR)").clicked() { self.execute_action_immediate(ctx, PendingAction::Ocr); }
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(ACCOUNT_VOICE).color(icon_color))).on_hover_text("Image to Speech").clicked() { self.execute_action_immediate(ctx, PendingAction::Speak); }
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(PRINT).color(icon_color))).on_hover_text("Print Selection (Ctrl+P)").clicked() { self.prepare_print_preview(ctx, selection); }
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(SAVE).color(icon_color))).on_hover_text("Save (Ctrl+S)").clicked() { self.execute_action_immediate(ctx, PendingAction::Save); }
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(COPY).color(icon_color))).on_hover_text("Copy (Ctrl+C)").clicked() { self.execute_action_immediate(ctx, PendingAction::Copy); }
                    if ui.add_sized([BTN_SIZE, BTN_SIZE], egui::Button::new(egui::RichText::new(CLOSE_THICK).color(icon_color))).on_hover_text("Close (Esc)").clicked() { self.deactivate(ctx); }
                });
            });

        if self.current_tool == Tool::Marker {
            let marker_bar_pos = egui::pos2(selection.max.x + spacing + 31.0, selection.max.y - 148.0 + 50.0);
            egui::Window::new("marker_settings").fixed_pos(marker_bar_pos).title_bar(false).resizable(false).collapsible(false).frame(egui::Frame::window(&ctx.style()).fill(toolbar_color).stroke(Stroke::new(1.0, Color32::GRAY)).inner_margin(4.0)).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let rect = ui.allocate_at_least(egui::vec2(14.0, 14.0), egui::Sense::hover()).0;
                    ui.painter().circle_filled(rect.center(), 7.0, self.current_color.gamma_multiply(self.marker_opacity));
                    ui.painter().circle_stroke(rect.center(), 7.0, Stroke::new(1.0, Color32::GRAY));
                    let response = ui.add(
                        egui::Slider::new(&mut self.marker_opacity, 0.1..=1.0)
                            .show_value(false)
                            .trailing_fill(true),
                    );
                    if response.changed() {
                        self.marker_opacity = self.marker_opacity.clamp(0.1, 1.0);
                        self.config.marker_opacity = self.marker_opacity;
                        {
                            let mut config = crate::config::cfg_mut();
                            *config = self.config.clone();
                        }
                        crate::config::save();
                    }
                });
            });
        }
    }
}
