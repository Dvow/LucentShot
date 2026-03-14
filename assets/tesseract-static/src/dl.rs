#![allow(non_camel_case_types, non_snake_case, non_upper_case_globals)]

use dlopen::raw::Library;
use std::os::raw::{c_char, c_int, c_uchar};
use lazy_static::lazy_static;
use std::fs;
use std::path::{Path, PathBuf};

const TESSERACT_LIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/tesseract.dll"));
const LEPTONICA_LIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/leptonica.dll"));

lazy_static! {
    static ref API: Api = {
        const LEPTONICA_FILENAME: &str = "leptonica-1.85.0.dll";
        const TESSERACT_FILENAME: &str = "tesseract.dll";

        let tempdir = std::env::temp_dir().join("lightshotv2_tesseract");

        fs::create_dir_all(&tempdir)
            .expect("Failed to create temp directory for Tesseract/Leptonica libraries");

        let leptonica_path: PathBuf = tempdir.join(LEPTONICA_FILENAME);
        let tesseract_path: PathBuf = tempdir.join(TESSERACT_FILENAME);

        fs::write(&leptonica_path, &LEPTONICA_LIB)
            .expect("Failed to write Leptonica library to disk");
        fs::write(&tesseract_path, &TESSERACT_LIB)
            .expect("Failed to write Tesseract library to disk");

        match init(&leptonica_path, &tesseract_path) {
            Ok(api) => api,
            Err(e) => panic!("Failed to init Tesseract/Leptonica API: {}", e),
        }
    };
}

pub fn get_api() -> &'static Api {
    let api = &API;
    std::hint::black_box((&api.leptonica_handle, &api.tesseract_handle));
    api
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TessBaseAPI {
    _unused: [u8; 0],
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ETEXT_DESC {
    _unused: [u8; 0],
}

pub struct Api {
    leptonica_handle: Library,
    tesseract_handle: Library,

    pub TessBaseAPIDelete: unsafe extern "C" fn(*mut TessBaseAPI),
    pub TessBaseAPICreate: unsafe extern "C" fn() -> *mut TessBaseAPI,
    pub TessBaseAPIInit3: unsafe extern "C" fn(*mut TessBaseAPI, *const c_char, *const c_char) -> c_int,
    pub TessBaseAPISetImage: unsafe extern "C" fn(*mut TessBaseAPI, *const c_uchar, c_int, c_int, c_int, c_int),
    pub TessBaseAPISetSourceResolution: unsafe extern "C" fn(*mut TessBaseAPI, c_int),
    pub TessBaseAPISetVariable: unsafe extern "C" fn(*mut TessBaseAPI, *const c_char, *const c_char) -> c_int,
    pub TessBaseAPIRecognize: unsafe extern "C" fn(*mut TessBaseAPI, *mut ETEXT_DESC) -> c_int,
    pub TessBaseAPIGetUTF8Text: unsafe extern "C" fn(*mut TessBaseAPI) -> *mut c_char,
    pub TessDeleteText: unsafe extern "C" fn(*const c_char),
}

fn init(leptonica_path: &Path, tesseract_path: &Path) -> Result<Api, String> {
    let leptonica_handle = Library::open(leptonica_path)
        .map_err(|e| format!("Failed to open Leptonica library at {}: {}", leptonica_path.display(), e))?;

    let tesseract_handle = Library::open(tesseract_path)
        .map_err(|e| format!("Failed to open Tesseract library at {}: {}", tesseract_path.display(), e))?;

    Ok(Api {
        TessBaseAPIDelete: unsafe { tesseract_handle.symbol("TessBaseAPIDelete") }
            .map_err(|e| format!("Failed to load TessBaseAPIDelete: {}", e))?,
        TessBaseAPICreate: unsafe { tesseract_handle.symbol("TessBaseAPICreate") }
            .map_err(|e| format!("Failed to load TessBaseAPICreate: {}", e))?,
        TessBaseAPIInit3: unsafe { tesseract_handle.symbol("TessBaseAPIInit3") }
            .map_err(|e| format!("Failed to load TessBaseAPIInit3: {}", e))?,
        TessBaseAPISetImage: unsafe { tesseract_handle.symbol("TessBaseAPISetImage") }
            .map_err(|e| format!("Failed to load TessBaseAPISetImage: {}", e))?,
        TessBaseAPISetSourceResolution: unsafe { tesseract_handle.symbol("TessBaseAPISetSourceResolution") }
            .map_err(|e| format!("Failed to load TessBaseAPISetSourceResolution: {}", e))?,
        TessBaseAPISetVariable: unsafe { tesseract_handle.symbol("TessBaseAPISetVariable") }
            .map_err(|e| format!("Failed to load TessBaseAPISetVariable: {}", e))?,
        TessBaseAPIRecognize: unsafe { tesseract_handle.symbol("TessBaseAPIRecognize") }
            .map_err(|e| format!("Failed to load TessBaseAPIRecognize: {}", e))?,
        TessBaseAPIGetUTF8Text: unsafe { tesseract_handle.symbol("TessBaseAPIGetUTF8Text") }
            .map_err(|e| format!("Failed to load TessBaseAPIGetUTF8Text: {}", e))?,
        TessDeleteText: unsafe { tesseract_handle.symbol("TessDeleteText") }
            .map_err(|e| format!("Failed to load TessDeleteText: {}", e))?,

        leptonica_handle,
        tesseract_handle,
    })
}
