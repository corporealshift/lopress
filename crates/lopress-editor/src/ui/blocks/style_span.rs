use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate};
use floem::text::{Attrs, AttrsList, FamilyOwned, Style, Weight};
use floem::views::editor::id::EditorId;
use floem::views::editor::layout::TextLayoutLine;
use floem::views::editor::text::Styling;
use floem::views::editor::EditorStyle;

use crate::model::style_span::StyleSpan;

const LINK_COLOR: Color = Color::rgb8(0x22, 0x7C, 0xBB);
const MONO_FAMILY: &str = "monospace";

/// Implements Floem's `Styling` trait to apply `Vec<StyleSpan>` attributes
/// (bold/italic/code/link) to the native editor's text layout.
///
/// `text` is the full block text, kept in sync with the rope so
/// `apply_attr_styles` can compute line-start byte offsets for multi-line
/// blocks (blocks with `\n` from Shift+Enter soft breaks).
///
/// `rev` is bumped whenever `spans` changes, causing Floem to invalidate its
/// text-layout cache and re-run `apply_attr_styles`.
pub struct InlineRunStyling {
    pub spans: RwSignal<Vec<StyleSpan>>,
    pub text: RwSignal<String>,
    pub rev: RwSignal<u64>,
    pub font_size: usize,
}

impl Styling for InlineRunStyling {
    fn id(&self) -> u64 {
        self.rev.get_untracked()
    }

    fn font_size(&self, _edid: EditorId, _line: usize) -> usize {
        self.font_size
    }

    fn apply_attr_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        line: usize,
        default: Attrs,
        attrs: &mut AttrsList,
    ) {
        let spans = self.spans.get_untracked();
        let full_text = self.text.get_untracked();

        // Compute byte offset of the start of logical line `line`.
        // Logical lines are delimited by '\n' (inserted by Shift+Enter).
        let line_start: usize = full_text
            .split('\n')
            .take(line)
            .map(|l| l.len() + 1) // +1 for the '\n' byte
            .sum();
        let line_len: usize = full_text.split('\n').nth(line).map(str::len).unwrap_or(0);
        let line_end = line_start + line_len;
        // Allocated once per apply_attr_styles call, not once per span.
        let mono_family = [FamilyOwned::Name(MONO_FAMILY.into())];

        for span in &spans {
            // Skip spans that don't overlap this logical line.
            if span.end <= line_start || span.start >= line_end {
                continue;
            }

            // Clip to line boundaries and convert to line-relative offsets.
            let local_start = span.start.saturating_sub(line_start);
            let local_end = span.end.min(line_end) - line_start;
            if local_start >= local_end {
                continue;
            }

            let mut a = default;
            if span.bold {
                a = a.weight(Weight::BOLD);
            }
            if span.italic {
                a = a.style(Style::Italic);
            }
            if span.code {
                a = a.family(&mono_family);
            }
            if span.link.is_some() {
                a = a.color(LINK_COLOR);
            }
            attrs.add_span(local_start..local_end, a);
        }
    }

    fn apply_layout_styles(
        &self,
        _edid: EditorId,
        _style: &EditorStyle,
        _line: usize,
        _layout_line: &mut TextLayoutLine,
    ) {
        // No layout-level overrides needed for inline styling.
    }
}

impl InlineRunStyling {
    /// Bump the revision counter so Floem's text-layout cache is invalidated.
    /// Call this after mutating `spans`.
    pub fn bump_rev(&self) {
        self.rev.update(|r| *r = r.wrapping_add(1));
    }
}
