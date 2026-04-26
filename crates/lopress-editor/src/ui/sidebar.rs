use crate::state::EditingState;
pub enum SidebarAction {
    None,
    SelectDocument(lopress_gui_host::DocumentRef),
    OpenPreview,
}
pub fn show(_ui: &mut egui::Ui, _state: &mut EditingState) -> SidebarAction {
    SidebarAction::None
}
