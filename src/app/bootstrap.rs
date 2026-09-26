use crate::error::AppError;

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
compile_error!("airslate-pc-server supports only Windows, macOS, and Linux");

pub fn validate() -> Result<(), AppError> {
    if !matches!(std::env::consts::OS, "windows" | "macos" | "linux") {
        return Err(AppError::Startup(
            "expected to run on Windows, macOS, or Linux",
        ));
    }

    Ok(())
}
