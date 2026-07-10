use crate::error::AppError;

pub fn validate() -> Result<(), AppError> {
    if !matches!(std::env::consts::OS, "windows" | "macos") {
        return Err(AppError::Startup("expected to run on Windows or macOS"));
    }

    Ok(())
}
