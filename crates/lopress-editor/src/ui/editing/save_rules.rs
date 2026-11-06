//! The decisions behind saving, closing, switching and refreshing, with no UI.

use lopress_gui_host::DiskState;

/// What a save request should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStep {
    NothingToDo,
    /// The file changed on disk, so writing would replace those changes.
    ConfirmOverwrite,
    Write,
}

/// A clean post writes nothing. A missing file is written back.
pub fn decide_save(unsaved: bool, disk: DiskState) -> SaveStep {
    match (unsaved, disk) {
        (false, _) => SaveStep::NothingToDo,
        (true, DiskState::Changed) => SaveStep::ConfirmOverwrite,
        (true, DiskState::Unchanged | DiskState::Missing) => SaveStep::Write,
    }
}

/// What closing the window should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseStep {
    Nothing,
    Save,
}

/// The close can't be stopped or confirmed, so a file that changed on disk
/// wins: losing unsaved edits is better than replacing the newer file.
pub fn decide_close(unsaved: bool, disk: DiskState) -> CloseStep {
    match (unsaved, disk) {
        (true, DiskState::Unchanged | DiskState::Missing) => CloseStep::Save,
        (false, _) | (true, DiskState::Changed) => CloseStep::Nothing,
    }
}

/// What moving to another document should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchStep {
    Proceed,
    Confirm,
}

pub fn decide_switch(unsaved: bool) -> SwitchStep {
    if unsaved {
        SwitchStep::Confirm
    } else {
        SwitchStep::Proceed
    }
}

/// What the Refresh button should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshStep {
    /// Nothing new to load: the file matches, or it's gone.
    Unavailable,
    Reload,
    /// Reloading would discard unsaved edits.
    ConfirmDiscard,
}

pub fn decide_refresh(unsaved: bool, disk: DiskState) -> RefreshStep {
    match (disk, unsaved) {
        (DiskState::Unchanged | DiskState::Missing, _) => RefreshStep::Unavailable,
        (DiskState::Changed, false) => RefreshStep::Reload,
        (DiskState::Changed, true) => RefreshStep::ConfirmDiscard,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DiskState::{Changed, Missing, Unchanged};

    #[test]
    fn a_clean_post_never_writes() {
        for disk in [Unchanged, Changed, Missing] {
            assert_eq!(decide_save(false, disk), SaveStep::NothingToDo);
        }
    }

    #[test]
    fn an_unsaved_post_writes_unless_the_file_changed() {
        assert_eq!(decide_save(true, Unchanged), SaveStep::Write);
        assert_eq!(decide_save(true, Missing), SaveStep::Write);
        assert_eq!(decide_save(true, Changed), SaveStep::ConfirmOverwrite);
    }

    #[test]
    fn closing_a_clean_post_does_nothing() {
        for disk in [Unchanged, Changed, Missing] {
            assert_eq!(decide_close(false, disk), CloseStep::Nothing);
        }
    }

    #[test]
    fn closing_saves_unless_the_file_changed() {
        assert_eq!(decide_close(true, Unchanged), CloseStep::Save);
        assert_eq!(decide_close(true, Missing), CloseStep::Save);
        assert_eq!(decide_close(true, Changed), CloseStep::Nothing);
    }

    #[test]
    fn switching_asks_only_when_unsaved() {
        assert_eq!(decide_switch(false), SwitchStep::Proceed);
        assert_eq!(decide_switch(true), SwitchStep::Confirm);
    }

    #[test]
    fn refresh_needs_a_changed_file() {
        for unsaved in [false, true] {
            assert_eq!(decide_refresh(unsaved, Unchanged), RefreshStep::Unavailable);
            assert_eq!(decide_refresh(unsaved, Missing), RefreshStep::Unavailable);
        }
    }

    #[test]
    fn refresh_asks_before_discarding() {
        assert_eq!(decide_refresh(false, Changed), RefreshStep::Reload);
        assert_eq!(decide_refresh(true, Changed), RefreshStep::ConfirmDiscard);
    }
}
