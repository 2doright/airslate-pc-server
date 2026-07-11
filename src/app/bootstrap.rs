use crate::error::AppError;

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("airslate-pc-server supports only Windows and macOS");

pub fn validate() -> Result<(), AppError> {
    if !matches!(std::env::consts::OS, "windows" | "macos") {
        return Err(AppError::Startup("expected to run on Windows or macOS"));
    }

    Ok(())
}
