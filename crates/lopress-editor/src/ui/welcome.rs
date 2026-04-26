use crate::state::WelcomeState;
pub enum WelcomeAction {
    None,
    OpenPicker,
    OpenPath(std::path::PathBuf),
}
pub fn show(_ui: &mut egui::Ui, _state: &mut WelcomeState) -> WelcomeAction {
    WelcomeAction::None
}
