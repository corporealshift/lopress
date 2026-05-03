pub mod state;
pub mod ui;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Floem launch failed: {0}")]
    Launch(String),
}

/// Run the editor app. Returns when the window closes.
///
/// # Errors
/// Returns `AppError::Launch` if the Floem runtime fails to start.
pub fn run() -> Result<(), AppError> {
    floem::launch(ui::root_view);
    Ok(())
}
