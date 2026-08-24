#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

use dlopen::raw::Library;
use std::fs;
use std::os::raw::{c_char, c_int, c_uchar};
use std::path::Path;
use std::sync::OnceLock;

const TESSERACT_LIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tesseract.dll"));
const LEPTONICA_LIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/leptonica.dll"));

pub fn get_api() -> &'static Api {
    static API: OnceLock<Api> = OnceLock::new();
    API.get_or_init(|| {
        const LEPTONICA_FILENAME: &str = "leptonica-1.85.0.dll";
        const TESSERACT_FILENAME: &str = "tesseract.dll";

        let tempdir = std::env::temp_dir().join("lightshotv2_tesseract");
        fs::create_dir_all(&tempdir)
            .expect("Failed to create temp directory for Tesseract/Leptonica libraries");

        let leptonica_path = tempdir.join(LEPTONICA_FILENAME);
        let tesseract_path = tempdir.join(TESSERACT_FILENAME);
        fs::write(&leptonica_path, LEPTONICA_LIB)
            .expect("Failed to write Leptonica library to disk");
        fs::write(&tesseract_path, TESSERACT_LIB)
            .expect("Failed to write Tesseract library to disk");

        init(&leptonica_path, &tesseract_path)
            .unwrap_or_else(|e| panic!("Failed to init Tesseract/Leptonica API: {e}"))
    })
}

#[repr(C)]
pub struct TessBaseAPI {
    _unused: [u8; 0],
}

#[repr(C)]
pub struct ETEXT_DESC {
    _unused: [u8; 0],
}

pub struct Api {
    _leptonica_handle: Library,
    _tesseract_handle: Library,
    pub TessBaseAPIDelete: unsafe extern "C" fn(*mut TessBaseAPI),
    pub TessBaseAPICreate: unsafe extern "C" fn() -> *mut TessBaseAPI,
    pub TessBaseAPIInit3: unsafe extern "C" fn(*mut TessBaseAPI, *const c_char, *const c_char) -> c_int,
    pub TessBaseAPISetImage:
        unsafe extern "C" fn(*mut TessBaseAPI, *const c_uchar, c_int, c_int, c_int, c_int),
    pub TessBaseAPISetSourceResolution: unsafe extern "C" fn(*mut TessBaseAPI, c_int),
    pub TessBaseAPISetVariable:
        unsafe extern "C" fn(*mut TessBaseAPI, *const c_char, *const c_char) -> c_int,
    pub TessBaseAPIRecognize: unsafe extern "C" fn(*mut TessBaseAPI, *mut ETEXT_DESC) -> c_int,
    pub TessBaseAPIGetUTF8Text: unsafe extern "C" fn(*mut TessBaseAPI) -> *mut c_char,
    pub TessDeleteText: unsafe extern "C" fn(*const c_char),
}

fn init(leptonica_path: &Path, tesseract_path: &Path) -> Result<Api, String> {
    let leptonica_handle = Library::open(leptonica_path).map_err(|e| {
        format!(
            "Failed to open Leptonica library at {}: {e}",
            leptonica_path.display()
        )
    })?;
    let tesseract_handle = Library::open(tesseract_path).map_err(|e| {
        format!(
            "Failed to open Tesseract library at {}: {e}",
            tesseract_path.display()
        )
    })?;

    Ok(Api {
        TessBaseAPIDelete: load(&tesseract_handle, "TessBaseAPIDelete")?,
        TessBaseAPICreate: load(&tesseract_handle, "TessBaseAPICreate")?,
        TessBaseAPIInit3: load(&tesseract_handle, "TessBaseAPIInit3")?,
        TessBaseAPISetImage: load(&tesseract_handle, "TessBaseAPISetImage")?,
        TessBaseAPISetSourceResolution: load(&tesseract_handle, "TessBaseAPISetSourceResolution")?,
        TessBaseAPISetVariable: load(&tesseract_handle, "TessBaseAPISetVariable")?,
        TessBaseAPIRecognize: load(&tesseract_handle, "TessBaseAPIRecognize")?,
        TessBaseAPIGetUTF8Text: load(&tesseract_handle, "TessBaseAPIGetUTF8Text")?,
        TessDeleteText: load(&tesseract_handle, "TessDeleteText")?,
        _leptonica_handle: leptonica_handle,
        _tesseract_handle: tesseract_handle,
    })
}

fn load<T>(lib: &Library, name: &str) -> Result<T, String> {
    // SAFETY: `name` is a documented Tesseract C API symbol with the requested signature.
    unsafe {
        lib.symbol(name)
            .map_err(|e| format!("Failed to load {name}: {e}"))
    }
}
