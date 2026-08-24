use anyhow::{anyhow, Result};
use image::{DynamicImage, RgbaImage};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

pub struct RawCapture {
    pub width: i32,
    pub height: i32,
    pub pixels: Vec<u8>,
}

pub fn primary_screen_size() -> (i32, i32) {
    // SAFETY: GetSystemMetrics is always safe for these documented screen indexes.
    unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
}

pub fn virtual_screen_bounds() -> (i32, i32, i32, i32) {
    // SAFETY: GetSystemMetrics is always safe for these documented virtual-screen indexes.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        )
    }
}

pub fn capture_primary_screen(include_cursor: bool) -> Result<RawCapture> {
    let (vx, vy, width, height) = virtual_screen_bounds();
    if width <= 0 || height <= 0 {
        return Err(anyhow!("Virtual screen has no area"));
    }
    let session = GdiSession::new(width, height)?;
    session.blit(0, 0, width, height, vx, vy)?;
    if include_cursor {
        session.draw_cursor(vx, vy);
    }
    session.read_pixels()
}

pub fn capture_focused_window() -> Result<RawCapture> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect};

    // SAFETY: Querying the foreground window and its frame bounds only reads window metrics.
    let (left, top, width, height) = unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return Err(anyhow!("No foreground window"));
        }
        let mut rect = RECT::default();
        let use_dwm = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok();
        if !use_dwm {
            GetWindowRect(hwnd, &mut rect)?;
            rect.left += 2;
            rect.top += 2;
            rect.right = (rect.right - 2).max(rect.left);
            rect.bottom = (rect.bottom - 2).max(rect.top);
        }
        let left = rect.left + 1;
        let top = rect.top + 1;
        let right = (rect.right - 1).max(left);
        let bottom = (rect.bottom - 1).max(top);
        let width = (right - left).max(1);
        let height = (bottom - top).max(1);
        (left, top, width, height)
    };

    let session = GdiSession::new(width, height)?;
    session.blit(0, 0, width, height, left, top)?;
    session.read_pixels()
}

pub fn to_dynamic_image(raw: RawCapture) -> DynamicImage {
    let (width, height, pixels) = bgra_to_rgba(raw);
    DynamicImage::ImageRgba8(
        RgbaImage::from_raw(width, height, pixels).expect("BGRA buffer matches capture size"),
    )
}

fn bgra_to_rgba(raw: RawCapture) -> (u32, u32, Vec<u8>) {
    let mut pixels = raw.pixels;
    for chunk in pixels.as_chunks_mut::<4>().0 {
        chunk.swap(0, 2);
        chunk[3] = 255;
    }
    (raw.width as u32, raw.height as u32, pixels)
}

struct GdiSession {
    screen_dc: HDC,
    mem_dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    width: i32,
    height: i32,
}

impl GdiSession {
    fn new(width: i32, height: i32) -> Result<Self> {
        // SAFETY: Compatible-DC creation uses process-owned GDI objects; failures are checked
        // before the session is returned, and Drop releases every handle.
        unsafe {
            let screen_dc = GetDC(HWND(0));
            if screen_dc.is_invalid() {
                return Err(anyhow!("GetDC failed"));
            }
            let mem_dc = CreateCompatibleDC(screen_dc);
            if mem_dc.is_invalid() {
                ReleaseDC(HWND(0), screen_dc);
                return Err(anyhow!("CreateCompatibleDC failed"));
            }
            let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
            if bitmap.is_invalid() {
                let _ = DeleteDC(mem_dc);
                ReleaseDC(HWND(0), screen_dc);
                return Err(anyhow!("CreateCompatibleBitmap failed"));
            }
            let old = SelectObject(mem_dc, bitmap);
            Ok(Self {
                screen_dc,
                mem_dc: HDC(mem_dc.0),
                bitmap,
                old,
                width,
                height,
            })
        }
    }

    fn blit(
        &self,
        dest_x: i32,
        dest_y: i32,
        width: i32,
        height: i32,
        src_x: i32,
        src_y: i32,
    ) -> Result<()> {
        // SAFETY: source and destination DCs belong to this session and stay alive for the blit.
        unsafe {
            BitBlt(
                self.mem_dc,
                dest_x,
                dest_y,
                width,
                height,
                self.screen_dc,
                src_x,
                src_y,
                SRCCOPY,
            )?;
        }
        Ok(())
    }

    fn draw_cursor(&self, origin_x: i32, origin_y: i32) {
        use windows::Win32::UI::WindowsAndMessaging::{
            DrawIconEx, GetCursorInfo, GetIconInfo, CURSORINFO, CURSOR_SHOWING, DI_NORMAL,
        };

        // SAFETY: Cursor info and icon bitmaps are queried then released with DeleteObject.
        unsafe {
            let mut cursor_info = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                ..Default::default()
            };
            if GetCursorInfo(&mut cursor_info).is_err() || cursor_info.flags != CURSOR_SHOWING {
                return;
            }
            let mut icon_info = Default::default();
            if GetIconInfo(cursor_info.hCursor, &mut icon_info).is_err() {
                return;
            }
            let x = cursor_info.ptScreenPos.x - icon_info.xHotspot as i32 - origin_x;
            let y = cursor_info.ptScreenPos.y - icon_info.yHotspot as i32 - origin_y;
            let _ = DrawIconEx(
                self.mem_dc,
                x,
                y,
                cursor_info.hCursor,
                0,
                0,
                0,
                None,
                DI_NORMAL,
            );
            if !icon_info.hbmMask.is_invalid() {
                let _ = DeleteObject(icon_info.hbmMask);
            }
            if !icon_info.hbmColor.is_invalid() {
                let _ = DeleteObject(icon_info.hbmColor);
            }
        }
    }

    fn read_pixels(&self) -> Result<RawCapture> {
        // SAFETY: The bitmap is selected into mem_dc and the output buffer is sized for 32-bpp BGRA.
        unsafe {
            let mut bmi = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: self.width,
                biHeight: -self.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let mut pixels = vec![0u8; (self.width * self.height * 4) as usize];
            let copied = GetDIBits(
                self.mem_dc,
                self.bitmap,
                0,
                self.height as u32,
                Some(pixels.as_mut_ptr().cast()),
                (&mut bmi as *mut BITMAPINFOHEADER).cast(),
                DIB_RGB_COLORS,
            );
            if copied == 0 {
                return Err(anyhow!("GetDIBits failed"));
            }
            Ok(RawCapture {
                width: self.width,
                height: self.height,
                pixels,
            })
        }
    }
}

impl Drop for GdiSession {
    fn drop(&mut self) {
        // SAFETY: These handles were created in `new` and are released exactly once.
        unsafe {
            let _ = SelectObject(self.mem_dc, self.old);
            let _ = DeleteObject(self.bitmap);
            let _ = DeleteDC(self.mem_dc);
            ReleaseDC(HWND(0), self.screen_dc);
        }
    }
}
