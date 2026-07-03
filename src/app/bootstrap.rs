use crate::error::AppError;

pub fn validate() -> Result<(), AppError> {
    if std::env::consts::OS != "windows" {
        return Err(AppError::Startup("expected to run on Windows"));
    }

    Ok(())
}
