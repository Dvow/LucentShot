use crate::dl::{get_api, TessBaseAPI as TessSysBaseAPI};
use std::convert::TryInto;
use std::ffi::CStr;
use std::fmt;
use std::os::raw::{c_char, c_int};
use std::ptr;

#[derive(Debug)]
pub struct TessBaseApi(*mut TessSysBaseAPI);

// SAFETY: all application access is serialized behind a Mutex.
unsafe impl Send for TessBaseApi {}

impl Drop for TessBaseApi {
    fn drop(&mut self) {
        // SAFETY: pointer was created by TessBaseAPICreate and is unique to this wrapper.
        unsafe { (get_api().TessBaseAPIDelete)(self.0) }
    }
}

impl Default for TessBaseApi {
    fn default() -> Self {
        Self::create()
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
            Self::Init => "TessBaseApi failed to initialize",
            Self::SetVariable => "TessBaseApi failed to set variable",
            Self::Recognize => "TessBaseApi failed to recognize",
            Self::GetText => "TessBaseApi get_utf8_text returned null",
            Self::ImageTooLarge => "Image dimensions exceed computer memory",
            Self::ImageSizeMismatch => "Image dimensions exceed image size",
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
        unsafe { (get_api().TessDeleteText)(self.0) }
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
        Self(unsafe { (get_api().TessBaseAPICreate)() })
    }

    pub fn init_2(
        &mut self,
        datapath: Option<&CStr>,
        language: Option<&CStr>,
    ) -> Result<(), TessError> {
        // SAFETY: optional CStr pointers remain valid for the duration of this call.
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
        let claimed_image_size: usize = (height * bytes_per_line)
            .try_into()
            .map_err(|_| TessError::ImageTooLarge)?;
        if claimed_image_size > image_data.len() {
            return Err(TessError::ImageSizeMismatch);
        }
        if bytes_per_pixel == 0 {
            if width > bytes_per_line.saturating_mul(8) {
                return Err(TessError::StrideTooSmall);
            }
        } else if width.saturating_mul(bytes_per_pixel) > bytes_per_line {
            return Err(TessError::StrideTooSmall);
        }
        // SAFETY: buffer size was checked against width/height/stride.
        unsafe {
            (get_api().TessBaseAPISetImage)(
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
        unsafe {
            (get_api().TessBaseAPISetSourceResolution)(self.0, ppi);
        }
    }

    pub fn set_variable(&mut self, name: &CStr, value: &CStr) -> Result<(), TessError> {
        // SAFETY: name and value stay valid for this call.
        let ret = unsafe { (get_api().TessBaseAPISetVariable)(self.0, name.as_ptr(), value.as_ptr()) };
        if ret == 1 {
            Ok(())
        } else {
            Err(TessError::SetVariable)
        }
    }

    pub fn recognize(&mut self) -> Result<(), TessError> {
        // SAFETY: API pointer is valid; a null monitor is accepted by Tesseract.
        let ret = unsafe { (get_api().TessBaseAPIRecognize)(self.0, ptr::null_mut()) };
        if ret == 0 {
            Ok(())
        } else {
            Err(TessError::Recognize)
        }
    }

    pub fn get_utf8_text(&mut self) -> Result<Text, TessError> {
        // SAFETY: a non-null result is owned by the caller and freed by Text::drop.
        let ptr = unsafe { (get_api().TessBaseAPIGetUTF8Text)(self.0) };
        if ptr.is_null() {
            Err(TessError::GetText)
        } else {
            Ok(Text(ptr))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_variable_error_test() -> Result<(), Box<dyn std::error::Error>> {
        let fail = std::ffi::CString::new("fail")?;
        let mut tess = TessBaseApi::create();
        tess.init_2(None, None)?;
        assert!(tess.set_variable(&fail, &fail).is_err());
        Ok(())
    }
}
