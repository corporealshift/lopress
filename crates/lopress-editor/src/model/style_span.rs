/// Byte-offset span with inline style flags. `start` is inclusive, `end`
/// exclusive, both measured in UTF-8 bytes from the block's rope start.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<String>,
}

impl StyleSpan {
    pub fn plain(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            bold: false,
            italic: false,
            code: false,
            link: None,
        }
    }

    pub fn same_style(&self, other: &StyleSpan) -> bool {
        self.bold == other.bold
            && self.italic == other.italic
            && self.code == other.code
            && self.link == other.link
    }
}

/// Which inline attribute a toolbar/keyboard shortcut toggles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineFlag {
    Bold,
    Italic,
    Code,
    Link,
}

/// Split the span that straddles `abs` into two spans at that byte boundary.
/// No-op if `abs` falls on an existing span boundary or outside all spans.
pub fn split_span_at(spans: &mut Vec<StyleSpan>, abs: usize) {
    let Some(i) = spans.iter().position(|s| s.start < abs && abs < s.end) else {
        return;
    };
    let Some(span) = spans.get(i).cloned() else {
        return;
    };
    let left = StyleSpan {
        start: span.start,
        end: abs,
        bold: span.bold,
        italic: span.italic,
        code: span.code,
        link: span.link.clone(),
    };
    let right = StyleSpan {
        start: abs,
        end: span.end,
        bold: span.bold,
        italic: span.italic,
        code: span.code,
        link: span.link,
    };
    spans.splice(i..=i, [left, right]);
}

/// Merge adjacent spans that share the same style and are contiguous.
pub fn coalesce_spans(spans: &mut Vec<StyleSpan>) {
    let mut i = 0;
    while i + 1 < spans.len() {
        let merge = spans
            .get(i)
            .zip(spans.get(i + 1))
            .map(|(a, b)| a.end == b.start && a.same_style(b))
            .unwrap_or(false);
        if merge {
            let b_end = spans.get(i + 1).map(|s| s.end).unwrap_or(0);
            if let Some(a) = spans.get_mut(i) {
                a.end = b_end;
            }
            spans.remove(i + 1);
        } else {
            i += 1;
        }
    }
}

/// Toggle `flag` across `[sel_start, sel_end)` (byte offsets).
/// If every overlapping span already has the flag, clears it; otherwise sets.
/// A collapsed selection (`sel_start == sel_end`) is a no-op.
pub fn toggle_inline(
    spans: &mut Vec<StyleSpan>,
    sel_start: usize,
    sel_end: usize,
    flag: InlineFlag,
) {
    if sel_start >= sel_end || spans.is_empty() {
        return;
    }
    let all_set = spans
        .iter()
        .filter(|s| s.start < sel_end && s.end > sel_start)
        .all(|s| match flag {
            InlineFlag::Bold => s.bold,
            InlineFlag::Italic => s.italic,
            InlineFlag::Code => s.code,
            InlineFlag::Link => s.link.is_some(),
        });
    let new_value = !all_set;
    // Split higher boundary first so the lower index stays valid.
    split_span_at(spans, sel_end);
    split_span_at(spans, sel_start);
    for span in spans.iter_mut() {
        if span.start >= sel_start && span.end <= sel_end {
            match flag {
                InlineFlag::Bold => span.bold = new_value,
                InlineFlag::Italic => span.italic = new_value,
                InlineFlag::Code => span.code = new_value,
                InlineFlag::Link => {
                    span.link = if new_value { Some(String::new()) } else { None };
                }
            }
        }
    }
    coalesce_spans(spans);
}
