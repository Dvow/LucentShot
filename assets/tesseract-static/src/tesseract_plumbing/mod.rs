mod tess_base_api;
mod text;

pub use tess_base_api::{
    TessBaseApi, TessBaseApiGetUtf8TextError, TessBaseApiInitError, TessBaseApiRecogniseError,
    TessBaseApiSetImageSafetyError, TessBaseApiSetVariableError,
};
pub use text::Text;
