//! Toggle a markdown emphasis marker (`**`, `_`, `` ` ``) around a selection in
//! plain markdown text. Used by plugin textarea fields, whose values are stored
//! and rendered as markdown rather than as styled runs (#54).

use std::ops::Range;

/// One text replacement plus the selection to show afterwards. All offsets are
/// UTF-8 byte offsets into the text before (`range`) or after (`selection`)
/// the replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownEdit {
    pub range: Range<usize>,
    pub replacement: String,
    pub selection: Range<usize>,
}

/// Toggle `marker` around the selection `[start, end)` of `text`.
///
/// - Markers immediately outside the selection, or as its first and last
///   characters, are removed (unwrap).
/// - Otherwise the selection is wrapped. A collapsed selection gets an empty
///   pair with the caret between the markers.
///
/// Returns `None` when the offsets aren't valid char boundaries in `text`.
pub fn toggle_markdown_wrap(
    text: &str,
    start: usize,
    end: usize,
    marker: &str,
) -> Option<MarkdownEdit> {
    let (start, end) = (start.min(end), start.max(end));
    let selected = text.get(start..end)?;
    let m = marker.len();

    // Unwrap: markers sit just outside the selection.
    let outer_start = start.checked_sub(m);
    if let Some(os) = outer_start {
        if text.get(os..start) == Some(marker) && text.get(end..end + m) == Some(marker) {
            return Some(MarkdownEdit {
                range: os..end + m,
                replacement: selected.to_string(),
                selection: os..end - m,
            });
        }
    }

    // Unwrap: the selection itself starts and ends with the marker.
    if selected.len() >= 2 * m {
        if let Some(inner) = selected
            .strip_prefix(marker)
            .and_then(|s| s.strip_suffix(marker))
        {
            return Some(MarkdownEdit {
                range: start..end,
                replacement: inner.to_string(),
                selection: start..start + inner.len(),
            });
        }
    }

    // Wrap.
    Some(MarkdownEdit {
        range: start..end,
        replacement: format!("{marker}{selected}{marker}"),
        selection: start + m..end + m,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(text: &str, edit: &MarkdownEdit) -> String {
        let mut out = text.to_string();
        out.replace_range(edit.range.clone(), &edit.replacement);
        out
    }

    #[test]
    fn wraps_a_selection_and_keeps_it_selected() {
        let text = "make this bold";
        let edit = toggle_markdown_wrap(text, 10, 14, "**").unwrap();
        let out = apply(text, &edit);
        assert_eq!(out, "make this **bold**");
        assert_eq!(out.get(edit.selection), Some("bold"));
    }

    #[test]
    fn unwraps_when_markers_are_just_outside_the_selection() {
        let text = "make this **bold**";
        let edit = toggle_markdown_wrap(text, 12, 16, "**").unwrap();
        let out = apply(text, &edit);
        assert_eq!(out, "make this bold");
        assert_eq!(out.get(edit.selection), Some("bold"));
    }

    #[test]
    fn unwraps_when_the_selection_includes_the_markers() {
        let text = "make this **bold**";
        let edit = toggle_markdown_wrap(text, 10, 18, "**").unwrap();
        let out = apply(text, &edit);
        assert_eq!(out, "make this bold");
        assert_eq!(out.get(edit.selection), Some("bold"));
    }

    #[test]
    fn collapsed_caret_inserts_an_empty_pair_with_the_caret_inside() {
        let text = "ab";
        let edit = toggle_markdown_wrap(text, 1, 1, "_").unwrap();
        assert_eq!(apply(text, &edit), "a__b");
        assert_eq!(edit.selection, 2..2);
    }

    #[test]
    fn wraps_multibyte_text() {
        let text = "café au lait";
        let edit = toggle_markdown_wrap(text, 0, 5, "`").unwrap();
        let out = apply(text, &edit);
        assert_eq!(out, "`café` au lait");
        assert_eq!(out.get(edit.selection), Some("café"));
    }

    #[test]
    fn rejects_offsets_inside_a_multibyte_char() {
        // 'é' spans bytes 3..5.
        assert_eq!(toggle_markdown_wrap("café", 4, 5, "**"), None);
    }

    #[test]
    fn reversed_offsets_are_normalized() {
        let text = "make this bold";
        let edit = toggle_markdown_wrap(text, 14, 10, "**").unwrap();
        assert_eq!(apply(text, &edit), "make this **bold**");
    }
}
