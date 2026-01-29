use anyhow::Result;
use egui::ColorImage;

pub struct RawCapture {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}

#[cfg(target_os = "windows")]
pub fn capture_primary_screen_raw() -> Result<RawCapture> {
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, CreateCompatibleDC, CreateCompatibleBitmap, 
        SelectObject, BitBlt, DeleteDC, DeleteObject, GetDIBits,
        SRCCOPY, DIB_RGB_COLORS, BITMAPINFOHEADER, BI_RGB,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let width = GetSystemMetrics(SM_CXSCREEN);
        let height = GetSystemMetrics(SM_CYSCREEN);

        let h_dc_screen = GetDC(None);
        let h_dc_mem = CreateCompatibleDC(h_dc_screen);
        let h_bitmap = CreateCompatibleBitmap(h_dc_screen, width, height);
        let h_old_obj = SelectObject(h_dc_mem, h_bitmap);
        
        // HARDWARE BITBLT - FASTEST POSSIBLE CAPTURE
        BitBlt(h_dc_mem, 0, 0, width, height, h_dc_screen, 0, 0, SRCCOPY)?;

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
