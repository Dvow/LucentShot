use std::env;
use std::path::Path;

fn main() {
    let out_dir_s = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir_s);
    let lept_target = out_dir.join("leptonica.dll");
    let tess_target = out_dir.join("tesseract.dll");

    let manifest_dir_s = env::var("CARGO_MANIFEST_DIR").unwrap();
    let manifest_dir = Path::new(&manifest_dir_s);
    let project_root = manifest_dir.parent();
    if let Some(root) = project_root {
        let dlls_dir = root;
        let lept_local = dlls_dir.join("leptonica-1.85.0.dll");
        let tess_local = dlls_dir.join("tesseract.dll");
        if lept_local.exists() && tess_local.exists() {
            std::fs::copy(&lept_local, &lept_target).expect("Failed to copy leptonica DLL");
            std::fs::copy(&tess_local, &tess_target).expect("Failed to copy tesseract DLL");
            return;
        }
    }

    eprintln!("Error: Tesseract/Leptonica DLLs not found in assets/ folder.");
    eprintln!("Required: assets/leptonica-1.85.0.dll and assets/tesseract.dll");
    eprintln!("No download — place the DLLs in assets/ before building.");
    std::process::exit(1);
}
