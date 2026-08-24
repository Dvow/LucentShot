use dlopen::raw::Library;
use std::ffi::CStr;
use std::fmt;
use std::os::raw::{c_char, c_int, c_uchar};
use std::path::Path;
use std::ptr;
use std::sync::OnceLock;

const TESSERACT_DLL: &[u8] = include_bytes!("../assets/tesseract.dll");
const LEPTONICA_DLL: &[u8] = include_bytes!("../assets/leptonica-1.85.0.dll");

struct TessHandle {
    _unused: [u8; 0],
}

struct Api {
    _leptonica: Library,
    _tesseract: Library,
    delete: unsafe extern "C" fn(*mut TessHandle),
    create: unsafe extern "C" fn() -> *mut TessHandle,
    init3: unsafe extern "C" fn(*mut TessHandle, *const c_char, *const c_char) -> c_int,
    set_image: unsafe extern "C" fn(*mut TessHandle, *const c_uchar, c_int, c_int, c_int, c_int),
    set_source_resolution: unsafe extern "C" fn(*mut TessHandle, c_int),
    set_variable: unsafe extern "C" fn(*mut TessHandle, *const c_char, *const c_char) -> c_int,
    recognize: unsafe extern "C" fn(*mut TessHandle, *mut ()) -> c_int,
    get_utf8_text: unsafe extern "C" fn(*mut TessHandle) -> *mut c_char,
    delete_text: unsafe extern "C" fn(*const c_char),
}

fn api() -> &'static Api {
    static API: OnceLock<Api> = OnceLock::new();
    API.get_or_init(|| {
        let dir = std::env::temp_dir().join("lightshotv2_tesseract");
        std::fs::create_dir_all(&dir).expect("Failed to create Tesseract temp directory");
        let leptonica = dir.join("leptonica-1.85.0.dll");
        let tesseract = dir.join("tesseract.dll");
        std::fs::write(&leptonica, LEPTONICA_DLL).expect("Failed to write Leptonica DLL");
        std::fs::write(&tesseract, TESSERACT_DLL).expect("Failed to write Tesseract DLL");
        load_api(&leptonica, &tesseract).unwrap_or_else(|e| panic!("Failed to load Tesseract: {e}"))
    })
}

fn load_api(leptonica: &Path, tesseract: &Path) -> Result<Api, String> {
    let leptonica_lib =
        Library::open(leptonica).map_err(|e| format!("open {}: {e}", leptonica.display()))?;
    let tesseract_lib =
        Library::open(tesseract).map_err(|e| format!("open {}: {e}", tesseract.display()))?;
    Ok(Api {
        delete: symbol(&tesseract_lib, "TessBaseAPIDelete")?,
        create: symbol(&tesseract_lib, "TessBaseAPICreate")?,
        init3: symbol(&tesseract_lib, "TessBaseAPIInit3")?,
        set_image: symbol(&tesseract_lib, "TessBaseAPISetImage")?,
        set_source_resolution: symbol(&tesseract_lib, "TessBaseAPISetSourceResolution")?,
        set_variable: symbol(&tesseract_lib, "TessBaseAPISetVariable")?,
        recognize: symbol(&tesseract_lib, "TessBaseAPIRecognize")?,
        get_utf8_text: symbol(&tesseract_lib, "TessBaseAPIGetUTF8Text")?,
        delete_text: symbol(&tesseract_lib, "TessDeleteText")?,
        _leptonica: leptonica_lib,
        _tesseract: tesseract_lib,
    })
}

fn symbol<T>(lib: &Library, name: &str) -> Result<T, String> {
    // SAFETY: `name` is a documented Tesseract C API symbol with the requested signature.
    unsafe { lib.symbol(name).map_err(|e| format!("load {name}: {e}")) }
}

pub struct TessBaseApi(*mut TessHandle);

// SAFETY: all application access is serialized behind a Mutex.
unsafe impl Send for TessBaseApi {}

impl Drop for TessBaseApi {
    fn drop(&mut self) {
        // SAFETY: pointer was created by TessBaseAPICreate and is unique to this wrapper.
        unsafe { (api().delete)(self.0) }
    }
}

#[derive(Debug)]
pub enum TessError {
    Init,
    SetVariable,
    Recognize,
    GetText,
    ImageTooLarge,
    ImageSizeMismatch,
    StrideTooSmall,
}

impl fmt::Display for TessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Init => "Tesseract failed to initialize",
            Self::SetVariable => "Tesseract failed to set variable",
            Self::Recognize => "Tesseract failed to recognize",
            Self::GetText => "Tesseract returned no text pointer",
            Self::ImageTooLarge => "Image dimensions exceed memory",
            Self::ImageSizeMismatch => "Image dimensions exceed buffer",
            Self::StrideTooSmall => "Image width exceeds bytes per line",
        })
    }
}

impl std::error::Error for TessError {}

pub struct Text(*mut c_char);

// SAFETY: Text is only moved across threads while the owning API is locked.
unsafe impl Send for Text {}

impl Drop for Text {
    fn drop(&mut self) {
        // SAFETY: pointer came from TessBaseAPIGetUTF8Text and is freed exactly once.
        unsafe { (api().delete_text)(self.0) }
    }
}

impl AsRef<CStr> for Text {
    fn as_ref(&self) -> &CStr {
        // SAFETY: Tesseract returns a valid NUL-terminated string.
        unsafe { CStr::from_ptr(self.0) }
    }
}

impl TessBaseApi {
    pub fn create() -> Self {
        // SAFETY: TessBaseAPICreate allocates a new API object.
        Self(unsafe { (api().create)() })
    }

    pub fn init_2(
        &mut self,
        datapath: Option<&CStr>,
        language: Option<&CStr>,
    ) -> Result<(), TessError> {
        // SAFETY: optional CStr pointers remain valid for the duration of this call.
        let ret = unsafe {
            (api().init3)(
                self.0,
                datapath.map(CStr::as_ptr).unwrap_or_else(ptr::null),
                language.map(CStr::as_ptr).unwrap_or_else(ptr::null),
            )
        };
        if ret == 0 {
            Ok(())
        } else {
            Err(TessError::Init)
        }
    }

    pub fn set_image(
        &mut self,
        image_data: &[u8],
        width: c_int,
        height: c_int,
        bytes_per_pixel: c_int,
        bytes_per_line: c_int,
    ) -> Result<(), TessError> {
        let claimed = height
            .checked_mul(bytes_per_line)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or(TessError::ImageTooLarge)?;
        if claimed > image_data.len() {
            return Err(TessError::ImageSizeMismatch);
        }
        let min_stride = if bytes_per_pixel == 0 {
            width.saturating_add(7) / 8
        } else {
            width.saturating_mul(bytes_per_pixel)
        };
        if min_stride > bytes_per_line {
            return Err(TessError::StrideTooSmall);
        }
        // SAFETY: buffer size was checked against width/height/stride.
        unsafe {
            (api().set_image)(
                self.0,
                image_data.as_ptr(),
                width,
                height,
                bytes_per_pixel,
                bytes_per_line,
            );
        }
        Ok(())
    }

    pub fn set_source_resolution(&mut self, ppi: c_int) {
        // SAFETY: API pointer is valid for the lifetime of self.
        unsafe { (api().set_source_resolution)(self.0, ppi) }
    }

    pub fn set_variable(&mut self, name: &CStr, value: &CStr) -> Result<(), TessError> {
        // SAFETY: name and value stay valid for this call.
        let ret = unsafe { (api().set_variable)(self.0, name.as_ptr(), value.as_ptr()) };
        if ret == 1 {
            Ok(())
        } else {
            Err(TessError::SetVariable)
        }
    }

    pub fn recognize(&mut self) -> Result<(), TessError> {
        // SAFETY: a null monitor is accepted by Tesseract.
        let ret = unsafe { (api().recognize)(self.0, ptr::null_mut()) };
        if ret == 0 {
            Ok(())
        } else {
            Err(TessError::Recognize)
        }
    }

    pub fn get_utf8_text(&mut self) -> Result<Text, TessError> {
        // SAFETY: a non-null result is owned by the caller and freed by Text::drop.
        let ptr = unsafe { (api().get_utf8_text)(self.0) };
        if ptr.is_null() {
            Err(TessError::GetText)
        } else {
            Ok(Text(ptr))
        }
    }
}
