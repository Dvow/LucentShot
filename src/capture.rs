use anyhow::Result;
use eframe::egui::ColorImage;

pub struct RawCapture {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}

/// Virtual screen bounds: (x, y, width, height) covering all monitors.
#[cfg(target_os = "windows")]
pub fn get_virtual_screen_bounds() -> (i32, i32, i32, i32) {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        (x, y, w, h)
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_virtual_screen_bounds() -> (i32, i32, i32, i32) {
    (0, 0, 1920, 1080)
}

#[cfg(target_os = "windows")]
pub fn capture_primary_screen_raw(include_cursor: bool) -> Result<RawCapture> {
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, CreateCompatibleDC, CreateCompatibleBitmap, 
        SelectObject, BitBlt, DeleteDC, DeleteObject, GetDIBits,
        SRCCOPY, DIB_RGB_COLORS, BITMAPINFOHEADER, BI_RGB,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DrawIconEx, GetCursorInfo, GetIconInfo, GetSystemMetrics, CURSORINFO, CURSOR_SHOWING,
        DI_NORMAL, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    unsafe {
        let vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let h_dc_screen = GetDC(None);
        let h_dc_mem = CreateCompatibleDC(h_dc_screen);
        let h_bitmap = CreateCompatibleBitmap(h_dc_screen, width, height);
        let h_old_obj = SelectObject(h_dc_mem, h_bitmap);
        
        // Capture the full virtual screen (all monitors)
        BitBlt(h_dc_mem, 0, 0, width, height, h_dc_screen, vx, vy, SRCCOPY)?;

        if include_cursor {
            let mut cursor_info = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            if GetCursorInfo(&mut cursor_info).is_ok()
                && cursor_info.flags == CURSOR_SHOWING
            {
                let mut icon_info = Default::default();
                if GetIconInfo(cursor_info.hCursor, &mut icon_info).is_ok() {
                    let x = cursor_info.ptScreenPos.x - icon_info.xHotspot as i32 - vx;
                    let y = cursor_info.ptScreenPos.y - icon_info.yHotspot as i32 - vy;
                    let _ = DrawIconEx(h_dc_mem, x, y, cursor_info.hCursor, 0, 0, 0, None, DI_NORMAL);
                    if icon_info.hbmMask.0 != 0 {
                        let _ = DeleteObject(icon_info.hbmMask);
                    }
                    if icon_info.hbmColor.0 != 0 {
                        let _ = DeleteObject(icon_info.hbmColor);
                    }
                }
            }
        }

        let mut bmi = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // negative for top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        };

        // Pre-allocate buffer to avoid reallocation during capture
        let mut buffer: Vec<u8> = vec![0; (width * height * 4) as usize];
        
        GetDIBits(
            h_dc_mem,
            h_bitmap,
            0,
            height as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            (&mut bmi as *mut BITMAPINFOHEADER) as *mut _,
            DIB_RGB_COLORS,
        );

        // CLEANUP
        let _ = SelectObject(h_dc_mem, h_old_obj);
        let _ = DeleteObject(h_bitmap);
        let _ = DeleteDC(h_dc_mem);
        ReleaseDC(None, h_dc_screen);

        Ok(RawCapture { width, height, pixels: buffer })
    }
}

#[cfg(target_os = "windows")]
pub fn capture_focused_window_raw() -> Result<RawCapture> {
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, CreateCompatibleDC, CreateCompatibleBitmap,
        SelectObject, BitBlt, DeleteDC, DeleteObject, GetDIBits,
        SRCCOPY, DIB_RGB_COLORS, BITMAPINFOHEADER, BI_RGB,
    };
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};
    use windows::Win32::Foundation::RECT;

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return Err(anyhow::anyhow!("No foreground window"));
        }
        let mut rect = RECT::default();
        // Prefer DWM extended frame bounds (excludes shadow) for consistent edges
        let use_dwm = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok();
        if !use_dwm {
            GetWindowRect(hwnd, &mut rect)?;
            // Fallback: inset by 2 pixels to reduce shadow/background bleed
            rect.left += 2;
            rect.top += 2;
            rect.right = (rect.right - 2).max(rect.left);
            rect.bottom = (rect.bottom - 2).max(rect.top);
        }
// Trim 1 pixel on each side around the entire edge
        let left = rect.left + 1;
        let top = rect.top + 1;
        let right = (rect.right - 1).max(left);
        let bottom = (rect.bottom - 1).max(top);
        let width = (right - left).max(1);
        let height = (bottom - top).max(1);
        if width <= 0 || height <= 0 {
            return Err(anyhow::anyhow!("Window has no area"));
        }
        let h_dc_screen = GetDC(None);
        let h_dc_mem = CreateCompatibleDC(h_dc_screen);
        let h_bitmap = CreateCompatibleBitmap(h_dc_screen, width, height);
        let h_old_obj = SelectObject(h_dc_mem, h_bitmap);
        BitBlt(
            h_dc_mem,
            0,
            0,
            width,
            height,
            h_dc_screen,
            left,
            top,
            SRCCOPY,
        )?;
        let mut bmi = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0 as u32,
            ..Default::default()
        };
        let mut buffer: Vec<u8> = vec![0; (width * height * 4) as usize];
        GetDIBits(
            h_dc_mem,
            h_bitmap,
            0,
            height as u32,
            Some(buffer.as_mut_ptr() as *mut _),
            (&mut bmi as *mut BITMAPINFOHEADER) as *mut _,
            DIB_RGB_COLORS,
        );
        let _ = SelectObject(h_dc_mem, h_old_obj);
        let _ = DeleteObject(h_bitmap);
        let _ = DeleteDC(h_dc_mem);
        ReleaseDC(None, h_dc_screen);
        Ok(RawCapture {
            width,
            height,
            pixels: buffer,
        })
    }
}

#[cfg(not(target_os = "windows"))]
pub fn capture_focused_window_raw() -> Result<RawCapture> {
    Err(anyhow::anyhow!("Focused window capture is Windows-only"))
}

pub fn raw_to_color_image(raw: RawCapture) -> ColorImage {
    // Parallel optimized swap: BGRA to RGBA
    let mut pixels = raw.pixels;
    
    // Using simple loop but could be par_chunks if needed.
    // For single screen, this is usually sub-millisecond.
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
        // Ensure alpha is 255
        chunk[3] = 255;
    }

    ColorImage::from_rgba_unmultiplied(
        [raw.width as usize, raw.height as usize],
        &pixels,
    )
}
