use crate::actions::Export;
use crate::config::Config;
use crate::draw::{self, Shape, Tool};
use crate::hotkey::{HotkeyEvent, HotkeyHandle};
use crate::settings::SettingsState;
use eframe::egui::{self, Color32, Id, Key, Pos2, Rect, Stroke, ViewportCommand};
use image::DynamicImage;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::thread;

pub const MENU_ID_QUIT: &str = "menu_quit";
pub const MENU_ID_SETTINGS: &str = "menu_settings";

pub struct OverlayApp {
    screenshot: Option<Arc<DynamicImage>>,
    texture: Option<egui::TextureHandle>,
    selection: Option<Rect>,
    is_selecting: bool,
    resizing_node: Option<usize>,
    start_pos: Option<Pos2>,
    current_tool: Tool,
    current_color: Color32,
    shapes: Vec<Shape>,
    redo_stack: Vec<Shape>,
    active_shape: Option<Shape>,
    is_active: bool,
    trigger_flag: Arc<AtomicBool>,
    settings_flag: Arc<AtomicBool>,
    marker_opacity: f32,
    rect_filled: bool,
    editing_text_index: Option<usize>,
    show_print_popup: bool,
    cropped_preview: Option<egui::TextureHandle>,
    printers: Vec<String>,
    selected_printer: String,
    print_copies: i32,
    print_landscape: bool,
    print_grayscale: bool,
    print_paper_size: String,
    print_fit_to_page: bool,
    show_settings: bool,
    ignore_close: bool,
    hide_after_paint: bool,
    settings_state: SettingsState,
    config: Config,
    hotkey_handle: HotkeyHandle,
    hotkey_rx: Receiver<HotkeyEvent>,
}

impl OverlayApp {
    pub fn new(
        trigger_flag: Arc<AtomicBool>,
        settings_flag: Arc<AtomicBool>,
        hotkey_handle: HotkeyHandle,
        hotkey_rx: Receiver<HotkeyEvent>,
    ) -> Self {
        let config = crate::config::get().clone();
        let color = config.drawing_color();
        Self {
            screenshot: None,
            texture: None,
            selection: None,
            is_selecting: false,
            resizing_node: None,
            start_pos: None,
            current_tool: Tool::Pen,
            current_color: color,
            shapes: Vec::new(),
            redo_stack: Vec::new(),
            active_shape: None,
            is_active: false,
            trigger_flag,
            settings_flag,
            marker_opacity: config.marker_opacity,
            rect_filled: false,
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
            ignore_close: false,
            hide_after_paint: false,
            settings_state: SettingsState::default(),
            config,
            hotkey_handle,
            hotkey_rx,
        }
    }

    fn persist(&self) {
        crate::config::persist(&self.config);
    }

    fn activate(&mut self, ctx: &egui::Context) {
        if self.show_settings {
            self.close_settings(ctx);
        }
        let Ok(raw) = crate::capture::capture_primary_screen(self.config.general_capture_cursor)
        else {
            return;
        };
        let image = crate::capture::to_dynamic_image(raw);
        self.texture = Some(texture_from_image(ctx, "screenshot", &image));
        self.screenshot = Some(Arc::new(image));
        self.hide_after_paint = false;
        self.reset_draw_state();
        if !self.config.general_keep_selected_area {
            self.selection = None;
        }
        self.current_tool = Tool::Pen;
        self.show_settings = false;
        self.is_active = true;
        egui::Popup::close_all(ctx);

        let (vx, vy, vw, vh) = crate::capture::virtual_screen_bounds();
        let ppp = ctx.pixels_per_point().max(0.1);
        send_cmds(
            ctx,
            [
                ViewportCommand::Fullscreen(false),
                ViewportCommand::Minimized(false),
                ViewportCommand::Resizable(false),
                ViewportCommand::MinInnerSize(egui::vec2(1.0, 1.0)),
                ViewportCommand::Decorations(false),
                ViewportCommand::Transparent(true),
                ViewportCommand::Title(crate::paths::APP_NAME.into()),
                ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop),
                ViewportCommand::OuterPosition(egui::pos2(vx as f32 / ppp, vy as f32 / ppp)),
                ViewportCommand::InnerSize(egui::vec2(vw as f32 / ppp, vh as f32 / ppp)),
                ViewportCommand::Visible(true),
                ViewportCommand::Focus,
                ViewportCommand::MousePassthrough(false),
            ],
        );
        force_present_window(vx, vy, vw, vh);
        ctx.request_repaint();
        self.hotkey_handle.set_listening(true);
    }

    fn reset_draw_state(&mut self) {
        self.reset_gesture();
        self.shapes.clear();
        self.redo_stack.clear();
        self.active_shape = None;
        self.editing_text_index = None;
        self.show_print_popup = false;
    }

    fn deactivate(&mut self, ctx: &egui::Context) {
        let needs_clean_frame = self.is_active
            && self.texture.is_some()
            && self.selection.is_some()
            && !self.config.general_keep_selected_area;
        self.reset_draw_state();
        if !self.config.general_keep_selected_area {
            self.selection = None;
        }
        if needs_clean_frame {
            self.hide_after_paint = true;
            ctx.request_repaint();
            return;
        }
        self.hide_overlay(ctx);
    }

    fn hide_overlay(&mut self, ctx: &egui::Context) {
        self.hide_after_paint = false;
        self.is_active = false;
        self.hotkey_handle.set_listening(false);
        self.texture = None;
        self.screenshot = None;
        self.cropped_preview = None;
        hide_main_window(ctx);
    }

    fn open_settings(&mut self, ctx: &egui::Context) {
        self.show_settings = true;
        self.ignore_close = true;
        self.hotkey_handle.set_listening(true);
        let size = egui::vec2(
            crate::settings::WINDOW_SIZE[0],
            crate::settings::WINDOW_SIZE[1],
        );
        let origin = settings_origin(ctx, size);
        send_cmds(
            ctx,
            [
                ViewportCommand::Fullscreen(false),
                ViewportCommand::Minimized(false),
                ViewportCommand::WindowLevel(egui::WindowLevel::Normal),
                ViewportCommand::MinInnerSize(egui::vec2(
                    crate::settings::WINDOW_MIN[0],
                    crate::settings::WINDOW_MIN[1],
                )),
                ViewportCommand::InnerSize(size),
                ViewportCommand::OuterPosition(origin),
                ViewportCommand::Decorations(true),
                ViewportCommand::Transparent(false),
                ViewportCommand::Resizable(true),
                ViewportCommand::Title("Settings".into()),
                ViewportCommand::Visible(true),
                ViewportCommand::MousePassthrough(false),
                ViewportCommand::Focus,
            ],
        );
        let ppp = ctx.pixels_per_point().max(0.1);
        force_present_window(
            (origin.x * ppp).round() as i32,
            (origin.y * ppp).round() as i32,
            (size.x * ppp).round() as i32,
            (size.y * ppp).round() as i32,
        );
        ctx.request_repaint();
    }

    fn close_settings(&mut self, ctx: &egui::Context) {
        self.show_settings = false;
        if !self.is_active {
            self.hotkey_handle.set_listening(false);
            hide_main_window(ctx);
        }
        ctx.request_repaint();
    }

    fn export(&mut self, ctx: &egui::Context, action: Export) {
        let Some(screenshot) = self.screenshot.as_ref().map(Arc::clone) else {
            return;
        };
        let Some(sel) = self.selection else {
            return;
        };
        let shapes = self.shapes.clone();
        let ppp = ctx.pixels_per_point();
        let auto_copy_link = self.config.general_auto_copy_link;
        let auto_close_upload = self.config.general_auto_close_upload;
        let notify = self.config.general_show_notifications;
        self.deactivate(ctx);

        thread::spawn(move || {
            let cropped = draw::rasterize_and_crop(&screenshot, &shapes, sel, ppp);
            run_export(action, &cropped, auto_copy_link, auto_close_upload, notify);
        });
    }

    fn instant_export(&self, action: Export) {
        let include_cursor = self.config.general_capture_cursor;
        let auto_copy_link = self.config.general_auto_copy_link;
        let auto_close_upload = self.config.general_auto_close_upload;
        let notify = self.config.general_show_notifications;
        thread::spawn(move || {
            let Ok(raw) = crate::capture::capture_primary_screen(include_cursor) else {
                return;
            };
            let img = crate::capture::to_dynamic_image(raw);
            run_export(action, &img, auto_copy_link, auto_close_upload, notify);
        });
    }

    fn copy_focused_window(&self) {
        let notify = self.config.general_show_notifications;
        thread::spawn(move || {
            let Ok(raw) = crate::capture::capture_focused_window() else {
                return;
            };
            let img = crate::capture::to_dynamic_image(raw);
            if crate::actions::copy_image(&img).is_ok() {
                crate::notification::maybe(notify, "Copy", "Copied to clipboard");
            }
        });
    }

    fn prepare_print(&mut self, ctx: &egui::Context, selection: Rect) {
        let Some(full_img) = self.screenshot.as_ref() else {
            return;
        };
        let processed =
            draw::rasterize_and_crop(full_img, &self.shapes, selection, ctx.pixels_per_point());
        self.cropped_preview = Some(texture_from_image(ctx, "print_preview", &processed));
        self.printers = crate::actions::printers();
        let known = self.printers.iter().any(|p| p == &self.selected_printer);
        if (self.selected_printer.is_empty() || !known)
            && let Some(first) = self.printers.first()
        {
            self.selected_printer = first.clone();
        }
        self.show_print_popup = true;
    }
}

fn send_cmds(ctx: &egui::Context, cmds: impl IntoIterator<Item = ViewportCommand>) {
    for cmd in cmds {
        ctx.send_viewport_cmd(cmd);
    }
}

fn hide_main_window(ctx: &egui::Context) {
    let (vx, vy, vw, vh) = crate::capture::virtual_screen_bounds();
    let ppp = ctx.pixels_per_point().max(0.1);
    send_cmds(
        ctx,
        [
            ViewportCommand::Fullscreen(false),
            ViewportCommand::Minimized(false),
            ViewportCommand::Resizable(false),
            ViewportCommand::WindowLevel(egui::WindowLevel::Normal),
            ViewportCommand::MousePassthrough(true),
            ViewportCommand::Decorations(false),
            ViewportCommand::Transparent(true),
            ViewportCommand::Title(crate::paths::APP_NAME.into()),
            ViewportCommand::OuterPosition(egui::pos2(vx as f32 / ppp, vy as f32 / ppp)),
            ViewportCommand::InnerSize(egui::vec2(vw as f32 / ppp, vh as f32 / ppp)),
            ViewportCommand::Visible(false),
        ],
    );
}

fn overlay_screen_rect(ctx: &egui::Context) -> Rect {
    let (_, _, vw, vh) = crate::capture::virtual_screen_bounds();
    let ppp = ctx.pixels_per_point().max(0.1);
    Rect::from_min_size(Pos2::ZERO, egui::vec2(vw as f32 / ppp, vh as f32 / ppp))
}

fn force_present_window(x: i32, y: i32, w: i32, h: i32) {
    let Some(hwnd) = find_app_hwnd() else {
        return;
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, SetWindowPos, ShowWindow, HWND_TOP, SWP_SHOWWINDOW, SW_RESTORE,
    };
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = SetWindowPos(hwnd, HWND_TOP, x, y, w.max(1), h.max(1), SWP_SHOWWINDOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn find_app_hwnd() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId,
    };

    struct Find {
        pid: u32,
        hwnd: HWND,
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let find = unsafe { &mut *(lparam.0 as *mut Find) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid != find.pid {
            return BOOL(1);
        }
        let mut buf = [0u16; 128];
        let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if n <= 0 {
            return BOOL(1);
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        if title == crate::paths::APP_NAME || title == "Settings" {
            find.hwnd = hwnd;
            return BOOL(0);
        }
        BOOL(1)
    }

    let mut find = Find {
        pid: unsafe { GetCurrentProcessId() },
        hwnd: windows::Win32::Foundation::HWND::default(),
    };
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut find as *mut Find as isize)) };
    (find.hwnd.0 != 0).then_some(find.hwnd)
}

fn settings_origin(ctx: &egui::Context, size: egui::Vec2) -> egui::Pos2 {
    let ppp = ctx.pixels_per_point().max(0.1);
    let (sw, sh) = crate::capture::primary_screen_size();
    let avail = egui::vec2(sw as f32 / ppp, sh as f32 / ppp);
    egui::pos2(
        ((avail.x - size.x) * 0.5).max(40.0),
        ((avail.y - size.y) * 0.5).max(40.0),
    )
}

fn texture_from_image(
    ctx: &egui::Context,
    name: impl Into<String>,
    image: &DynamicImage,
) -> egui::TextureHandle {
    let rgba = image.to_rgba8();
    ctx.load_texture(
        name,
        egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        ),
        Default::default(),
    )
}

fn run_export(
    action: Export,
    image: &DynamicImage,
    auto_copy_link: bool,
    auto_close_upload: bool,
    notify: bool,
) {
    let toast = |title: &str, body: &str| crate::notification::maybe(notify, title, body);
    let fail =
        |title: &str, err: &dyn std::fmt::Display| toast(title, &format!("{title} failed: {err}"));
    match action {
        Export::Copy => {
            if crate::actions::copy_image(image).is_ok() {
                toast("Copy", "Copied to clipboard");
            }
        }
        Export::Save => match crate::actions::save_image(image) {
            Ok(true) => toast("Save", "Image saved"),
            Ok(false) => {}
            Err(err) => fail("Save", &err),
        },
        Export::Upload => {
            match crate::actions::upload_prntsc(image).and_then(|url| {
                crate::actions::apply_upload_result(&url, auto_copy_link, auto_close_upload)
            }) {
                Ok(()) => toast("Upload", "Image uploaded"),
                Err(err) => fail("Upload", &err),
            }
        }
        Export::Ocr => match crate::ocr::image_to_text(image) {
            Ok(text) if crate::actions::copy_text(&text).is_ok() => {
                toast("OCR", "Text copied to clipboard");
            }
            Ok(_) => {}
            Err(err) => crate::ocr::show_error(&err.to_string()),
        },
        Export::Speak => {
            if let Err(err) = crate::ocr::image_to_speech(image) {
                crate::ocr::show_error(&format!("Image to Speech failed: {err}"));
            }
        }
        Export::ImageSearch => {
            if let Err(err) = crate::actions::image_search(image) {
                fail("Search", &err);
            }
        }
        Export::Print {
            printer,
            copies,
            landscape,
            grayscale,
            paper,
            fit,
        } => {
            if let Err(err) = crate::actions::print_image(
                image, &printer, copies, landscape, grayscale, &paper, fit,
            ) {
                fail("Print", &err);
            }
        }
    }
}

impl eframe::App for OverlayApp {
    fn logic(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll_external_events(ctx);
        if self.trigger_flag.swap(false, Ordering::SeqCst) && !self.is_active {
            self.activate(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;

        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            if self.ignore_close {
                self.ignore_close = false;
                return;
            }
            self.cancel_or_close(ctx);
            return;
        }
        self.ignore_close = false;

        if !self.is_active && !self.show_settings {
            hide_main_window(ctx);
            return;
        }

        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.cancel_or_close(ctx);
            return;
        }

        if self.is_active && ctx.input(|i| !i.focused && !i.pointer.any_down()) {
            self.reset_gesture();
        }

        if self.is_active && self.handle_shortcuts(ctx) {
            return;
        }

        if self.is_active {
            self.paint_overlay(ctx, ui);
            if self.hide_after_paint {
                self.hide_overlay(ctx);
            }
        }
        if self.show_settings {
            self.draw_settings(ui);
        }
    }
}

impl OverlayApp {
    fn poll_external_events(&mut self, ctx: &egui::Context) {
        if self.settings_flag.swap(false, Ordering::SeqCst) {
            self.open_settings(ctx);
        }

        while let Ok(event) = self.hotkey_rx.try_recv() {
            match event {
                HotkeyEvent::InstantSave => self.instant_export(Export::Save),
                HotkeyEvent::InstantUpload => self.instant_export(Export::Upload),
                HotkeyEvent::CopyFocusedWindow => self.copy_focused_window(),
            }
        }
    }

    fn cancel_or_close(&mut self, ctx: &egui::Context) {
        if self.show_settings {
            self.close_settings(ctx);
            return;
        }
        if self.is_active {
            self.deactivate(ctx);
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        let pressed = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(shortcut_from_event)
                .collect::<Vec<_>>()
        });

        for shortcut in &pressed {
            match shortcut {
                Shortcut::Undo => {
                    if let Some(shape) = self.shapes.pop() {
                        self.redo_stack.push(shape);
                    }
                }
                Shortcut::Redo => {
                    if let Some(shape) = self.redo_stack.pop() {
                        self.shapes.push(shape);
                    }
                }
                Shortcut::SelectAll => self.selection = Some(ctx.content_rect()),
                _ => {}
            }
        }

        let Some(sel) = self.normalized_selection() else {
            return false;
        };
        for shortcut in pressed {
            match shortcut {
                Shortcut::Copy => self.export(ctx, Export::Copy),
                Shortcut::Save => self.export(ctx, Export::Save),
                Shortcut::Upload => self.export(ctx, Export::Upload),
                Shortcut::ImageSearch => self.export(ctx, Export::ImageSearch),
                Shortcut::Print => {
                    self.prepare_print(ctx, sel);
                    return true;
                }
                _ => continue,
            }
            return true;
        }
        false
    }

    fn normalized_selection(&self) -> Option<Rect> {
        self.selection.map(|s| Rect::from_two_pos(s.min, s.max))
    }

    fn paint_overlay(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let screen_rect = overlay_screen_rect(ctx);
        ui.set_clip_rect(screen_rect);
        if let Some(texture) = &self.texture {
            ui.painter().image(
                texture.id(),
                screen_rect,
                Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }

        // Claim the overlay so the native window keeps pointer events. Gesture
        // state is driven from this-frame pointer input, not widget drag flags.
        let _ = ui.interact(
            screen_rect,
            Id::new("main_interact"),
            egui::Sense::click_and_drag(),
        );

        let pointer = latest_pointer_pos(ctx, screen_rect);
        let current_sel = self.normalized_selection();
        self.paint_selection_chrome(ui, screen_rect, current_sel, pointer);
        self.paint_shapes(ui);
        self.paint_text_editor(ctx);

        if let Some(sel) = current_sel
            && self.should_show_toolbars(ctx, sel)
        {
            self.show_toolbars(ctx, sel);
        }

        if let Some(pointer) = pointer {
            self.handle_pointer(ctx, pointer, current_sel);
        } else if ctx.input(|i| i.pointer.primary_released()) {
            self.finish_drag();
        }

        if self.show_print_popup {
            self.draw_print_window(ctx);
        }
    }

    fn paint_selection_chrome(
        &self,
        ui: &mut egui::Ui,
        screen_rect: Rect,
        current_sel: Option<Rect>,
        pointer: Option<Pos2>,
    ) {
        let Some(sel) = current_sel else {
            ui.painter()
                .rect_filled(screen_rect, 0.0, Color32::from_black_alpha(180));
            if self.hide_after_paint {
                return;
            }
            let Some(pointer) = pointer else {
                return;
            };
            ui.painter().text(
                pointer + egui::vec2(15.0, 15.0),
                egui::Align2::LEFT_TOP,
                "Select an area",
                egui::FontId::proportional(16.0),
                Color32::WHITE,
            );
            return;
        };

        paint_dimmed_outside(ui, screen_rect, sel);
        ui.painter().rect_stroke(
            sel,
            0.0,
            Stroke::new(1.0_f32, Color32::WHITE),
            egui::StrokeKind::Middle,
        );
        ui.painter().text(
            sel.left_top() - egui::vec2(0.0, 20.0),
            egui::Align2::LEFT_TOP,
            format!("{} x {}", sel.width().round(), sel.height().round()),
            egui::FontId::proportional(14.0),
            Color32::WHITE,
        );
        let hovered = pointer.and_then(|pos| hit_resize_handle(sel, pos));
        for (i, node) in resize_handles(sel).iter().enumerate() {
            let color = if self.resizing_node == Some(i) {
                Color32::LIGHT_BLUE
            } else if hovered == Some(i) {
                Color32::from_rgb(180, 220, 255)
            } else {
                Color32::WHITE
            };
            ui.painter().rect_filled(*node, 0.0, color);
        }
    }

    fn paint_shapes(&self, ui: &mut egui::Ui) {
        for (idx, shape) in self.shapes.iter().enumerate() {
            if Some(idx) == self.editing_text_index {
                continue;
            }
            draw::paint_shape(ui.painter(), shape);
        }
        if let Some(shape) = &self.active_shape {
            draw::paint_shape(ui.painter(), shape);
        }
    }

    fn paint_text_editor(&mut self, ctx: &egui::Context) {
        let Some(idx) = self.editing_text_index else {
            return;
        };
        let Some(shape) = self.shapes.get_mut(idx) else {
            return;
        };
        let Some(pos) = shape.points.first().copied() else {
            return;
        };
        let text_color = shape.color;
        let mut close = false;
        egui::Area::new(Id::new(("edit_text", idx)))
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let re = ui.add(
                    egui::TextEdit::singleline(&mut shape.text)
                        .font(egui::FontId::proportional(20.0))
                        .text_color(text_color)
                        .frame(egui::Frame::NONE),
                );
                re.request_focus();
                close = re.lost_focus() || ctx.input(|i| i.key_pressed(Key::Enter));
            });
        if close {
            self.editing_text_index = None;
        }
    }

    fn should_show_toolbars(&self, ctx: &egui::Context, sel: Rect) -> bool {
        let min_pts = 5.0 / ctx.pixels_per_point();
        sel.width() >= min_pts
            && sel.height() >= min_pts
            && !self.is_selecting
            && self.resizing_node.is_none()
            && !self.show_print_popup
    }

    fn handle_pointer(&mut self, ctx: &egui::Context, pointer: Pos2, current_sel: Option<Rect>) {
        let (pressed, down, released) = ctx.input(|i| {
            (
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
            )
        });

        if self.show_print_popup {
            if released {
                self.finish_drag();
            }
            return;
        }

        if pressed && !pointer_over_overlay_chrome(ctx, pointer) {
            self.begin_pointer_gesture(pointer, current_sel);
        }
        if down && self.has_active_gesture() {
            self.update_pointer_gesture(pointer);
        }
        if released {
            self.finish_drag();
        }
    }

    fn has_active_gesture(&self) -> bool {
        self.is_selecting || self.resizing_node.is_some() || self.active_shape.is_some()
    }

    fn begin_pointer_gesture(&mut self, pointer: Pos2, current_sel: Option<Rect>) {
        let Some(sel) = current_sel else {
            self.begin_selection(pointer);
            return;
        };
        if let Some(idx) = hit_resize_handle(sel, pointer) {
            self.resizing_node = Some(idx);
            return;
        }
        if !sel.contains(pointer) {
            self.begin_selection(pointer);
            return;
        }
        if self.current_tool == Tool::Text {
            self.redo_stack.clear();
            self.shapes
                .push(Shape::text_label(pointer, self.current_color));
            self.editing_text_index = Some(self.shapes.len() - 1);
            return;
        }
        self.active_shape = Some(Shape::stroke(
            self.current_tool,
            pointer,
            self.current_color,
            self.marker_opacity,
            self.rect_filled,
        ));
    }

    fn update_pointer_gesture(&mut self, pointer: Pos2) {
        if self.is_selecting {
            let Some(start) = self.start_pos else {
                return;
            };
            self.selection = Some(Rect::from_two_pos(start, pointer));
            return;
        }
        if let Some(idx) = self.resizing_node {
            let Some(sel) = self.selection else {
                return;
            };
            self.selection = Some(resize_selection(sel, idx, pointer));
            return;
        }
        let Some(shape) = &mut self.active_shape else {
            return;
        };
        let min_dist = if shape.is_marker() {
            (shape.stroke_width * 0.35).max(2.0)
        } else {
            1.0
        };
        let far_enough = shape
            .points
            .last()
            .is_none_or(|last| pointer.distance(*last) >= min_dist);
        if far_enough {
            shape.points.push(pointer);
        }
    }

    fn begin_selection(&mut self, pointer: Pos2) {
        self.is_selecting = true;
        self.start_pos = Some(pointer);
        self.selection = Some(Rect::from_two_pos(pointer, pointer));
    }

    fn finish_drag(&mut self) {
        self.is_selecting = false;
        self.resizing_node = None;
        self.start_pos = None;
        if let Some(shape) = self.active_shape.take() {
            self.redo_stack.clear();
            self.shapes.push(shape);
        }
        if let Some(sel) = self.selection {
            self.selection = Some(Rect::from_two_pos(sel.min, sel.max));
        }
    }

    fn reset_gesture(&mut self) {
        self.is_selecting = false;
        self.start_pos = None;
        self.resizing_node = None;
        self.active_shape = None;
    }

    fn draw_print_window(&mut self, ctx: &egui::Context) {
        egui::Window::new("Print")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_width(400.0);
                ui.vertical_centered(|ui| {
                    let before = self.print_snapshot();
                    ui.heading("Print Selection");
                    ui.add_space(10.0);
                    if let Some(texture) = &self.cropped_preview {
                        let max_size = egui::vec2(380.0, 200.0);
                        let tex_size = texture.size_vec2();
                        let factor = (max_size.x / tex_size.x)
                            .min(max_size.y / tex_size.y)
                            .min(1.0);
                        ui.add(egui::Image::new(texture).max_size(tex_size * factor));
                    }
                    ui.add_space(15.0);
                    egui::Grid::new("print_grid")
                        .num_columns(2)
                        .spacing([10.0, 10.0])
                        .show(ui, |ui| {
                            ui.label("Printer:");
                            egui::ComboBox::from_id_salt("printer_select")
                                .selected_text(&self.selected_printer)
                                .width(250.0)
                                .show_ui(ui, |ui| {
                                    for printer in &self.printers {
                                        ui.selectable_value(
                                            &mut self.selected_printer,
                                            printer.clone(),
                                            printer,
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label("Orientation:");
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.print_landscape, false, "Portrait");
                                ui.radio_value(&mut self.print_landscape, true, "Landscape");
                            });
                            ui.end_row();
                            ui.label("Color Mode:");
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.print_grayscale, false, "Color");
                                ui.radio_value(&mut self.print_grayscale, true, "Grayscale");
                            });
                            ui.end_row();
                            ui.label("Copies:");
                            ui.add(egui::DragValue::new(&mut self.print_copies).range(1..=99));
                            ui.end_row();
                            ui.label("Paper Size:");
                            egui::ComboBox::from_id_salt("paper_size")
                                .selected_text(&self.print_paper_size)
                                .show_ui(ui, |ui| {
                                    for size in ["A4", "Letter", "Legal"] {
                                        ui.selectable_value(
                                            &mut self.print_paper_size,
                                            size.to_string(),
                                            size,
                                        );
                                    }
                                });
                            ui.end_row();
                            ui.label("Scaling:");
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut self.print_fit_to_page, true, "Fit to Page");
                                ui.radio_value(&mut self.print_fit_to_page, false, "Actual Size");
                            });
                            ui.end_row();
                        });
                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        let btn_width = (ui.available_width() - 10.0) / 2.0;
                        if ui
                            .add_sized([btn_width, 30.0], egui::Button::new("Print"))
                            .clicked()
                        {
                            self.export(
                                ctx,
                                Export::Print {
                                    printer: self.selected_printer.clone(),
                                    copies: self.print_copies,
                                    landscape: self.print_landscape,
                                    grayscale: self.print_grayscale,
                                    paper: self.print_paper_size.clone(),
                                    fit: self.print_fit_to_page,
                                },
                            );
                        }
                        if ui
                            .add_sized([btn_width, 30.0], egui::Button::new("Cancel"))
                            .clicked()
                        {
                            self.show_print_popup = false;
                            self.cropped_preview = None;
                        }
                    });
                    if before != self.print_snapshot() {
                        self.persist_print_settings();
                    }
                });
            });
    }

    fn print_snapshot(&self) -> (String, i32, bool, bool, bool, String) {
        (
            self.selected_printer.clone(),
            self.print_copies,
            self.print_landscape,
            self.print_grayscale,
            self.print_fit_to_page,
            self.print_paper_size.clone(),
        )
    }

    fn persist_print_settings(&mut self) {
        self.config.print_selected_printer = self.selected_printer.clone();
        self.config.print_copies = self.print_copies;
        self.config.print_landscape = self.print_landscape;
        self.config.print_grayscale = self.print_grayscale;
        self.config.print_fit = self.print_fit_to_page;
        self.config.print_paper = self.print_paper_size.clone();
        self.persist();
    }

    fn draw_settings(&mut self, ui: &mut egui::Ui) {
        let before = self.config.clone();
        crate::settings::show(ui, &mut self.settings_state, &mut self.config);
        if before == self.config {
            return;
        }
        self.persist();
        self.hotkey_handle
            .update(crate::hotkey::HotkeyConfig::from_settings(&self.config));
    }

    fn show_toolbars(&mut self, ctx: &egui::Context, selection: Rect) {
        const BTN: f32 = 18.0;
        let bar = Color32::from_rgb(30, 30, 30);
        let frame = egui::Frame::window(&ctx.global_style())
            .fill(bar)
            .stroke(Stroke::new(1.0_f32, Color32::GRAY));
        use crate::icons::{self, Icon};

        chrome_window(
            "tools",
            egui::pos2(selection.max.x + 4.0, selection.max.y - 190.0),
            frame.inner_margin(2.0),
        )
        .fixed_size([BTN + 4.0, 240.0])
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                for (tool, icon, tip) in [
                    (Tool::Pen, Icon::Pen, "Pen"),
                    (Tool::Line, Icon::Line, "Line"),
                    (Tool::Arrow, Icon::Arrow, "Arrow"),
                ] {
                    icons::tool_toggle(ui, &mut self.current_tool, tool, icon, tip);
                }
                let (rect_icon, rect_tip) = if self.rect_filled {
                    (Icon::SquareFill, "Filled rectangle")
                } else {
                    (Icon::Square, "Rectangle outline")
                };
                icons::tool_toggle(ui, &mut self.current_tool, Tool::Rect, rect_icon, rect_tip);
                icons::tool_toggle(
                    ui,
                    &mut self.current_tool,
                    Tool::Marker,
                    Icon::Marker,
                    "Marker",
                );
                icons::tool_toggle(
                    ui,
                    &mut self.current_tool,
                    Tool::Text,
                    Icon::Text,
                    "Text Tool",
                );
                ui.separator();
                let color_response = ui
                    .scope(|ui| {
                        ui.spacing_mut().interact_size = egui::vec2(BTN, BTN);
                        ui.color_edit_button_srgba(&mut self.current_color)
                    })
                    .response
                    .on_hover_text("Change Color");
                if color_response.changed() {
                    self.config.set_drawing_color(self.current_color);
                    self.persist();
                }
                ui.add_space(2.0);
                if icons::icon_button(ui, Icon::Undo, "Undo (Ctrl+Z)")
                    && let Some(shape) = self.shapes.pop()
                {
                    self.redo_stack.push(shape);
                }
            });
        });

        chrome_window(
            "actions",
            egui::pos2(selection.max.x + 1.0, selection.max.y + 4.0),
            frame.inner_margin(4.0),
        )
        .pivot(egui::Align2::RIGHT_TOP)
        .auto_sized()
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if icons::icon_button(ui, Icon::Cloud, "Cloud Upload (Ctrl+D)") {
                    self.export(ctx, Export::Upload);
                }
                if icons::icon_button(ui, Icon::Search, "Image Search (Ctrl+G)") {
                    self.export(ctx, Export::ImageSearch);
                }
                #[cfg(feature = "ocr")]
                if icons::icon_button(ui, Icon::Ocr, "Image to Text (OCR)") {
                    self.export(ctx, Export::Ocr);
                }
                #[cfg(feature = "ocr")]
                if icons::icon_button(ui, Icon::Speak, "Image to Speech") {
                    self.export(ctx, Export::Speak);
                }
                if icons::icon_button(ui, Icon::Print, "Print Selection (Ctrl+P)") {
                    self.prepare_print(ctx, selection);
                }
                if icons::icon_button(ui, Icon::Save, "Save (Ctrl+S)") {
                    self.export(ctx, Export::Save);
                }
                if icons::icon_button(ui, Icon::Copy, "Copy (Ctrl+C)") {
                    self.export(ctx, Export::Copy);
                }
                if icons::icon_button(ui, Icon::Close, "Close (Esc)") {
                    self.deactivate(ctx);
                }
            });
        });

        if self.current_tool == Tool::Rect {
            chrome_window(
                "rect_settings",
                egui::pos2(selection.max.x + 35.0, selection.max.y - 123.0),
                frame.inner_margin(4.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if icons::icon_selectable(
                        ui,
                        !self.rect_filled,
                        Icon::Square,
                        "Rectangle outline",
                    ) {
                        self.rect_filled = false;
                    }
                    if icons::icon_selectable(
                        ui,
                        self.rect_filled,
                        Icon::SquareFill,
                        "Filled rectangle",
                    ) {
                        self.rect_filled = true;
                    }
                });
            });
        }

        if self.current_tool == Tool::Marker {
            chrome_window(
                "marker_settings",
                egui::pos2(selection.max.x + 35.0, selection.max.y - 100.0),
                frame.inner_margin(4.0),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let rect = ui
                        .allocate_at_least(egui::vec2(14.0, 14.0), egui::Sense::hover())
                        .0;
                    ui.painter().circle_filled(
                        rect.center(),
                        7.0,
                        self.current_color.gamma_multiply(self.marker_opacity),
                    );
                    ui.painter().circle_stroke(
                        rect.center(),
                        7.0,
                        Stroke::new(1.0_f32, Color32::GRAY),
                    );
                    if ui
                        .add(
                            egui::Slider::new(&mut self.marker_opacity, 0.1..=1.0)
                                .show_value(false)
                                .trailing_fill(true),
                        )
                        .changed()
                    {
                        self.marker_opacity = self.marker_opacity.clamp(0.1, 1.0);
                        self.config.marker_opacity = self.marker_opacity;
                        self.persist();
                    }
                });
            });
        }
    }
}

fn chrome_window<'a>(title: &'a str, pos: Pos2, frame: egui::Frame) -> egui::Window<'a> {
    egui::Window::new(title)
        .fixed_pos(pos)
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(frame)
}

fn paint_dimmed_outside(ui: &mut egui::Ui, screen: Rect, sel: Rect) {
    let dim = Color32::from_black_alpha(180);
    for rect in [
        Rect::from_min_max(screen.min, egui::pos2(screen.max.x, sel.min.y)),
        Rect::from_min_max(egui::pos2(screen.min.x, sel.max.y), screen.max),
        Rect::from_min_max(
            egui::pos2(screen.min.x, sel.min.y),
            egui::pos2(sel.min.x, sel.max.y),
        ),
        Rect::from_min_max(
            egui::pos2(sel.max.x, sel.min.y),
            egui::pos2(screen.max.x, sel.max.y),
        ),
    ] {
        ui.painter().rect_filled(rect, 0.0, dim);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shortcut {
    Copy,
    Save,
    Undo,
    Redo,
    Upload,
    ImageSearch,
    Print,
    SelectAll,
}

fn shortcut_from_event(event: &egui::Event) -> Option<Shortcut> {
    if matches!(event, egui::Event::Copy) {
        return Some(Shortcut::Copy);
    }
    let egui::Event::Key {
        key,
        pressed: true,
        modifiers,
        ..
    } = event
    else {
        return None;
    };
    if !(modifiers.ctrl || modifiers.command) {
        return None;
    }
    match (*key, modifiers.shift) {
        (Key::C, _) => Some(Shortcut::Copy),
        (Key::S, _) => Some(Shortcut::Save),
        (Key::Z, false) => Some(Shortcut::Undo),
        (Key::Z, true) => Some(Shortcut::Redo),
        (Key::D, _) => Some(Shortcut::Upload),
        (Key::G, _) => Some(Shortcut::ImageSearch),
        (Key::P, _) => Some(Shortcut::Print),
        (Key::A, _) => Some(Shortcut::SelectAll),
        _ => None,
    }
}

fn latest_pointer_pos(ctx: &egui::Context, bounds: Rect) -> Option<Pos2> {
    let pos = ctx.input(|i| latest_pos_from_events(i.pointer.latest_pos(), &i.events))?;
    if bounds.width() < 2.0 || bounds.height() < 2.0 {
        return Some(pos);
    }
    Some(pos.clamp(bounds.min, bounds.max))
}

fn latest_pos_from_events(latest: Option<Pos2>, events: &[egui::Event]) -> Option<Pos2> {
    let mut pos = latest;
    for event in events {
        if let egui::Event::PointerMoved(moved) = event {
            pos = Some(*moved);
        }
    }
    pos
}

fn pointer_over_overlay_chrome(ctx: &egui::Context, pos: Pos2) -> bool {
    ctx.layer_id_at(pos)
        .is_some_and(|layer| is_foreground_layer(layer.order))
}

fn is_foreground_layer(order: egui::Order) -> bool {
    order != egui::Order::Background
}

fn resize_handles(rect: Rect) -> [Rect; 8] {
    let size = egui::vec2(8.0, 8.0);
    [
        Rect::from_center_size(rect.left_top(), size),
        Rect::from_center_size(rect.right_top(), size),
        Rect::from_center_size(rect.right_bottom(), size),
        Rect::from_center_size(rect.left_bottom(), size),
        Rect::from_center_size(egui::pos2(rect.center().x, rect.top()), size),
        Rect::from_center_size(egui::pos2(rect.right(), rect.center().y), size),
        Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom()), size),
        Rect::from_center_size(egui::pos2(rect.left(), rect.center().y), size),
    ]
}

fn hit_resize_handle(rect: Rect, pointer: Pos2) -> Option<usize> {
    resize_handles(rect)
        .iter()
        .position(|handle| handle.contains(pointer))
}

fn resize_selection(mut sel: Rect, handle: usize, pointer: Pos2) -> Rect {
    match handle {
        0 => sel.min = pointer,
        1 => {
            sel.max.x = pointer.x;
            sel.min.y = pointer.y;
        }
        2 => sel.max = pointer,
        3 => {
            sel.min.x = pointer.x;
            sel.max.y = pointer.y;
        }
        4 => sel.min.y = pointer.y,
        5 => sel.max.x = pointer.x,
        6 => sel.max.y = pointer.y,
        7 => sel.min.x = pointer.x,
        _ => {}
    }
    sel
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: f32, y: f32) -> Pos2 {
        Pos2::new(x, y)
    }

    #[test]
    fn drag_rect_works_in_every_direction() {
        let down_right = Rect::from_two_pos(pos(10.0, 10.0), pos(40.0, 30.0));
        assert_eq!(down_right.min, pos(10.0, 10.0));
        assert_eq!(down_right.max, pos(40.0, 30.0));

        let up_left = Rect::from_two_pos(pos(40.0, 30.0), pos(10.0, 10.0));
        assert_eq!(up_left.min, pos(10.0, 10.0));
        assert_eq!(up_left.max, pos(40.0, 30.0));

        let up_right = Rect::from_two_pos(pos(10.0, 30.0), pos(40.0, 10.0));
        assert_eq!(up_right.min, pos(10.0, 10.0));
        assert_eq!(up_right.max, pos(40.0, 30.0));
    }

    #[test]
    fn click_without_drag_is_a_zero_size_rect() {
        let start = pos(12.0, 8.0);
        let sel = Rect::from_two_pos(start, start);
        assert_eq!(sel.width(), 0.0);
        assert_eq!(sel.height(), 0.0);
        assert!(sel.contains(start));
    }

    #[test]
    fn resize_handles_follow_all_eight_nodes() {
        let sel = Rect::from_min_max(pos(0.0, 0.0), pos(20.0, 10.0));
        assert_eq!(
            resize_selection(sel, 0, pos(-4.0, -2.0)).min,
            pos(-4.0, -2.0)
        );
        assert_eq!(
            resize_selection(sel, 2, pos(30.0, 18.0)).max,
            pos(30.0, 18.0)
        );
        assert_eq!(resize_selection(sel, 5, pos(25.0, 3.0)).max.x, 25.0);
        assert_eq!(resize_selection(sel, 7, pos(-1.0, 3.0)).min.x, -1.0);
        assert_eq!(hit_resize_handle(sel, pos(0.0, 0.0)), Some(0));
        assert_eq!(hit_resize_handle(sel, pos(20.0, 10.0)), Some(2));
        assert_eq!(hit_resize_handle(sel, pos(10.0, 5.0)), None);
    }

    #[test]
    fn latest_pointer_uses_last_move_event() {
        let events = [
            egui::Event::PointerMoved(pos(1.0, 1.0)),
            egui::Event::PointerMoved(pos(9.0, 4.0)),
        ];
        assert_eq!(
            latest_pos_from_events(Some(pos(0.0, 0.0)), &events),
            Some(pos(9.0, 4.0))
        );
        assert_eq!(
            latest_pos_from_events(Some(pos(3.0, 3.0)), &[]),
            Some(pos(3.0, 3.0))
        );
    }

    #[test]
    fn background_layer_is_not_toolbar_chrome() {
        assert!(!is_foreground_layer(egui::Order::Background));
        assert!(is_foreground_layer(egui::Order::Middle));
        assert!(is_foreground_layer(egui::Order::Foreground));
        assert!(is_foreground_layer(egui::Order::Tooltip));
    }

    #[test]
    fn ctrl_key_maps_to_overlay_shortcuts() {
        let copy = egui::Event::Key {
            key: Key::C,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        };
        assert_eq!(shortcut_from_event(&copy), Some(Shortcut::Copy));
        assert_eq!(
            shortcut_from_event(&egui::Event::Copy),
            Some(Shortcut::Copy)
        );
        let plain = egui::Event::Key {
            key: Key::C,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(shortcut_from_event(&plain), None);
    }
}
