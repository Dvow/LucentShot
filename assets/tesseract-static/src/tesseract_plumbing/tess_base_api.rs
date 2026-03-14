use crate::dl::{get_api, TessBaseAPI as TessSysBaseAPI};
use super::text::Text;
use std::convert::TryInto;
use std::ffi::CStr;
use std::os::raw::c_int;
use std::ptr;
use thiserror::Error;

#[derive(Debug)]
pub struct TessBaseApi(*mut TessSysBaseAPI);

unsafe impl Send for TessBaseApi {}

impl Drop for TessBaseApi {
    fn drop(&mut self) {
        unsafe { (get_api().TessBaseAPIDelete)(self.0) }
    }
}

impl Default for TessBaseApi {
    fn default() -> Self {
        Self::create()
    }
}

#[derive(Debug, Error)]
#[error("TessBaseApi failed to initialize")]
pub struct TessBaseApiInitError();

#[derive(Debug, Error)]
#[error("TessBaseApi failed to set variable")]
pub struct TessBaseApiSetVariableError();

#[derive(Debug, Error)]
#[error("TessBaseApi failed to recognize")]
pub struct TessBaseApiRecogniseError();

#[derive(Debug, Error)]
#[error("TessBaseApi get_utf8_text returned null")]
pub struct TessBaseApiGetUtf8TextError();

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum TessBaseApiSetImageSafetyError {
    #[error("Image dimensions exceed computer memory")]
    DimensionsExceedMemory(),
    #[error("Image dimensions exceed image size")]
    DimensionsExceedImageSize(),
    #[error("Image width exceeds bytes per line")]
    ImageWidthExceedsBytesPerLine(),
}

impl TessBaseApi {
    pub fn create() -> Self {
        Self(unsafe { (get_api().TessBaseAPICreate)() })
    }

    pub fn init_2(
        &mut self,
        datapath: Option<&CStr>,
        language: Option<&CStr>,
    ) -> Result<(), TessBaseApiInitError> {
        let ret = unsafe {
            (get_api().TessBaseAPIInit3)(
                self.0,
                datapath.map(CStr::as_ptr).unwrap_or_else(ptr::null),
                language.map(CStr::as_ptr).unwrap_or_else(ptr::null),
            )
        };
        if ret == 0 {
            Ok(())
        } else {
            Err(TessBaseApiInitError {})
        }
    }

    pub fn set_image(
        &mut self,
        image_data: &[u8],
        width: c_int,
        height: c_int,
        bytes_per_pixel: c_int,
        bytes_per_line: c_int,
    ) -> Result<(), TessBaseApiSetImageSafetyError> {
        let claimed_image_size: usize = (height * bytes_per_line)
            .try_into()
            .map_err(|_| TessBaseApiSetImageSafetyError::DimensionsExceedMemory())?;
        if claimed_image_size > image_data.len() {
            return Err(TessBaseApiSetImageSafetyError::DimensionsExceedImageSize());
        }
        match bytes_per_pixel {
            0 => {
                if width > bytes_per_line * 8 {
                    return Err(TessBaseApiSetImageSafetyError::ImageWidthExceedsBytesPerLine());
                }
            }
            _ => {
                if width * bytes_per_pixel > bytes_per_line {
                    return Err(TessBaseApiSetImageSafetyError::ImageWidthExceedsBytesPerLine());
                }
            }
        }
        unsafe {
            (get_api().TessBaseAPISetImage)(
                self.0,
                image_data.as_ptr(),
                width,
                height,
                bytes_per_pixel,
                bytes_per_line,
            );
        };
        Ok(())
    }

    pub fn set_source_resolution(&mut self, ppi: c_int) {
        unsafe {
            (get_api().TessBaseAPISetSourceResolution)(self.0, ppi);
        }
    }

    pub fn set_variable(
        &mut self,
        name: &CStr,
        value: &CStr,
    ) -> Result<(), TessBaseApiSetVariableError> {
        let ret = unsafe { (get_api().TessBaseAPISetVariable)(self.0, name.as_ptr(), value.as_ptr()) };
        match ret {
            1 => Ok(()),
            _ => Err(TessBaseApiSetVariableError {}),
        }
    }

    pub fn recognize(&mut self) -> Result<(), TessBaseApiRecogniseError> {
        let ret = unsafe { (get_api().TessBaseAPIRecognize)(self.0, ptr::null_mut()) };
        match ret {
            0 => Ok(()),
            _ => Err(TessBaseApiRecogniseError {}),
        }
    }

    pub fn get_utf8_text(&mut self) -> Result<Text, TessBaseApiGetUtf8TextError> {
        let ptr = unsafe { (get_api().TessBaseAPIGetUTF8Text)(self.0) };
        if ptr.is_null() {
            Err(TessBaseApiGetUtf8TextError {})
        } else {
            Ok(unsafe { Text::new(ptr) })
        }
    }
}

#[test]
fn set_variable_error_test() -> Result<(), Box<dyn std::error::Error>> {
    let fail = std::ffi::CString::new("fail")?;
    let mut tess = TessBaseApi::create();
    tess.init_2(None, None)?;
    assert!(tess.set_variable(&fail, &fail).is_err());
    Ok(())
}
