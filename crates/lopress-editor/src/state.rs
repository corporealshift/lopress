use lopress_gui_host::{DocumentRef, LoadedDocument, Session};

pub enum AppState {
    Welcome(WelcomeState),
    Editing(Box<EditingState>),
}

#[derive(Default)]
pub struct WelcomeState {
    /// Non-empty when the previous Open attempt failed.
    pub error: Option<String>,
}

pub struct EditingState {
    pub session: Session,
    /// The currently open document, if any.
    pub current_doc: Option<LoadedDocument>,
    /// The DocumentRef for the currently open document.
    pub current_ref: Option<DocumentRef>,
    /// Non-empty when the parse-error fallback view is active.
    pub parse_error_raw: Option<String>,
    pub parse_error_msg: Option<String>,
    /// Which block index is focused in the editor.
    pub focused_block: Option<usize>,
}

impl EditingState {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            current_doc: None,
            current_ref: None,
            parse_error_raw: None,
            parse_error_msg: None,
            focused_block: None,
        }
    }

    /// Switch to a new document, flushing any pending save first.
    pub fn open_document(&mut self, doc_ref: &DocumentRef) {
        self.flush_current();
        match self.session.load_document(&doc_ref.path) {
            Ok(doc) => {
                self.current_doc = Some(doc);
                self.current_ref = Some(doc_ref.clone());
                self.parse_error_raw = None;
                self.parse_error_msg = None;
            }
            Err(lopress_gui_host::LoadError::Parse { raw, message, .. }) => {
                self.current_doc = None;
                self.current_ref = Some(doc_ref.clone());
                self.parse_error_raw = Some(raw);
                self.parse_error_msg = Some(message);
            }
            Err(e) => {
                self.current_doc = None;
                self.current_ref = Some(doc_ref.clone());
                self.parse_error_raw = None;
                self.parse_error_msg = Some(e.to_string());
            }
        }
    }

    /// Flush the current document synchronously if dirty.
    pub fn flush_current(&mut self) {
        let Some(doc) = &mut self.current_doc else {
            return;
        };
        if !doc.dirty {
            return;
        }
        match self.session.save(doc) {
            Ok(()) => doc.mark_clean(),
            Err(e) => doc.last_save_error = Some(e.to_string()),
        }
    }
}
