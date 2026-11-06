//! The Paper theme: every colour, text size, spacing value and corner radius in
//! the editor, plus the bundled fonts.
//!
//! `tests/theme_tests.rs` fails on a colour literal anywhere else under `ui/`.
//! See `docs/superpowers/specs/2026-11-06-paper-theme-design.md`.

use floem::peniko::Color;
use floem::style::{CursorColor, Style};
use floem::text::{FamilyOwned, Weight, FONT_SYSTEM};
use floem::views::editor::view::EditorViewClass;
use floem::views::editor::SelectionColor;
use floem::views::scroll::{self, ScrollCustomStyle};
use floem::views::{ButtonClass, CheckboxClass, PlaceholderTextClass, TextInputClass};

// ── Colours ─────────────────────────────────────────────────────────────────

/// Sidebar, inspector and footer.
pub const CHROME_BG: Color = Color::rgb8(0xEF, 0xEA, 0xE0);
/// Article column and welcome screen.
pub const PAGE_BG: Color = Color::rgb8(0xFB, 0xF8, 0xF2);
/// Notes drawer, and cards inside the article.
pub const DRAWER_BG: Color = Color::rgb8(0xF5, 0xF0, 0xE6);
/// Text inputs.
pub const FIELD_BG: Color = Color::rgb8(0xF8, 0xF4, 0xEB);
/// Toolbar, menus and dialogs.
pub const SURFACE: Color = Color::rgb8(0xFF, 0xFF, 0xFF);
/// The focused block.
pub const BLOCK_FOCUS_BG: Color = Color::rgb8(0xF3, 0xED, 0xE1);
/// The open document in the sidebar.
pub const ROW_ACTIVE_BG: Color = Color::rgb8(0xE2, 0xDA, 0xCA);
/// Hovered rows and quiet buttons.
pub const ROW_HOVER_BG: Color = Color::rgb8(0xE8, 0xE1, 0xD3);

/// Main text.
pub const TEXT: Color = Color::rgb8(0x26, 0x22, 0x1D);
/// Quiet buttons and the toolbar.
pub const TEXT_SOFT: Color = Color::rgb8(0x4F, 0x48, 0x3F);
/// Labels and the footer.
pub const TEXT_MUTED: Color = Color::rgb8(0x6B, 0x63, 0x58);
/// Placeholders.
pub const PLACEHOLDER: Color = Color::rgb8(0x6F, 0x66, 0x5A);
/// Text on a filled accent button.
pub const ON_ACCENT: Color = Color::rgb8(0xFF, 0xFF, 0xFF);

/// Drawer and footer rules.
pub const RULE: Color = Color::rgb8(0xDD, 0xD5, 0xC7);
/// Toolbar dividers and list rules.
pub const RULE_SOFT: Color = Color::rgb8(0xE6, 0xDF, 0xD2);

/// Primary button, Draft, active toggles and delete. Errors too: Paper has one
/// accent colour.
pub const ACCENT: Color = Color::rgb8(0xA6, 0x3A, 0x2A);
/// Primary button hover, and links.
pub const ACCENT_HOVER: Color = Color::rgb8(0x7E, 0x2A, 0x1D);
/// Pressed toolbar button and the disk bar.
pub const ACCENT_TINT: Color = Color::rgb8(0xF3, 0xE4, 0xDF);

/// Build status dot.
pub const OK: Color = Color::rgb8(0x4C, 0x7A, 0x4F);
/// Unsaved state.
pub const WARN: Color = Color::rgb8(0x8A, 0x5A, 0x12);

/// Text selection: `ACCENT` at 20%.
pub const SELECTION: Color = Color::rgba8(0xA6, 0x3A, 0x2A, 51);
/// Caret and selection in a `text_input`, which floem paints in one colour, with
/// the selection on top of the text. `ACCENT` at 50%, so it's strong enough for
/// a 1px caret and leaves selected text readable.
pub const INPUT_CURSOR: Color = Color::rgba8(0xA6, 0x3A, 0x2A, 128);
/// Toolbar, menu and dialog shadows.
pub const SHADOW: Color = Color::rgba8(60, 45, 25, 41);
/// Behind a dialog.
pub const SCRIM: Color = Color::rgba8(38, 34, 29, 64);

// ── Text ────────────────────────────────────────────────────────────────────

/// UI text size.
pub const UI: f32 = 13.0;
/// Small UI text: labels, the footer and the toolbar.
pub const UI_SMALL: f32 = 12.0;

/// Article paragraphs, lists and tables.
pub const SERIF_FAMILY: &str = "Newsreader";
/// Headings and the welcome title.
pub const DISPLAY_FAMILY: &str = "Newsreader Display";
/// All UI text. Set as the default sans-serif by `load_fonts`.
pub const SANS_FAMILY: &str = "Instrument Sans";

/// Article text width at full column width.
pub const TEXT_MEASURE: f32 = 620.0;

/// Where a text role's font comes from.
#[derive(Clone, Copy)]
pub enum RoleFamily {
    /// A bundled family, by name.
    Named(&'static str),
    /// The system monospace font.
    Monospace,
}

/// How one kind of article text is set. Block editors and read-only runs both
/// take their size, family and line height from here.
#[derive(Clone, Copy)]
pub struct TextRole {
    pub size: u16,
    /// A multiple of `size`.
    pub line_height: f32,
    pub family: RoleFamily,
}

impl TextRole {
    /// Size in the form floem's `Styling` wants.
    pub fn font_size(self) -> usize {
        usize::from(self.size)
    }

    pub fn font_size_px(self) -> f32 {
        f32::from(self.size)
    }

    /// Whole pixels, so lines stay evenly spaced down a block.
    pub fn line_height_px(self) -> f32 {
        (self.font_size_px() * self.line_height).round()
    }

    pub fn families(self) -> Vec<FamilyOwned> {
        match self.family {
            RoleFamily::Named(name) => vec![FamilyOwned::Name(name.to_string())],
            RoleFamily::Monospace => vec![FamilyOwned::Monospace],
        }
    }

    /// Inline code runs use the system monospace, which sits larger than the
    /// serif, so they're set a little smaller to keep lines evenly spaced.
    pub fn code_run_size_px(self) -> f32 {
        self.font_size_px() * CODE_RUN_SCALE
    }
}

const CODE_RUN_SCALE: f32 = 0.88;

const fn serif(size: u16, line_height: f32) -> TextRole {
    TextRole {
        size,
        line_height,
        family: RoleFamily::Named(SERIF_FAMILY),
    }
}

const fn display(size: u16, line_height: f32) -> TextRole {
    TextRole {
        size,
        line_height,
        family: RoleFamily::Named(DISPLAY_FAMILY),
    }
}

/// Paragraphs, list items and table cells.
pub const BODY: TextRole = serif(19, 1.6);
pub const H1: TextRole = display(40, 1.15);
pub const H2: TextRole = display(25, 1.25);
pub const H3: TextRole = display(21, 1.3);
pub const H4: TextRole = display(19, 1.3);
pub const H5: TextRole = display(17, 1.3);
pub const H6: TextRole = display(15, 1.3);
/// Code blocks.
pub const CODE: TextRole = TextRole {
    size: 14,
    line_height: 1.5,
    family: RoleFamily::Monospace,
};

/// The role for a heading level. Anything past 6 is set as 6.
pub const fn heading_role(level: u8) -> TextRole {
    match level {
        1 => H1,
        2 => H2,
        3 => H3,
        4 => H4,
        5 => H5,
        _ => H6,
    }
}

// ── Spacing and corners ─────────────────────────────────────────────────────

pub const SPACE_2: f32 = 2.0;
pub const SPACE_4: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_8: f32 = 8.0;
pub const SPACE_10: f32 = 10.0;
pub const SPACE_12: f32 = 12.0;
pub const SPACE_16: f32 = 16.0;
pub const SPACE_18: f32 = 18.0;
pub const SPACE_24: f32 = 24.0;

/// Toolbar buttons.
pub const CORNER_SM: f32 = 5.0;
/// Controls and rows.
pub const CORNER_MD: f32 = 6.0;
/// Toolbar, menus and dialogs.
pub const CORNER_LG: f32 = 8.0;

// ── Global styling ──────────────────────────────────────────────────────────

/// Scrollbar handle thickness.
const SCROLL_HANDLE: f32 = 6.0;

/// The look of every control, set once on the root view. A view's own style
/// still wins over these.
pub fn root_style(s: Style) -> Style {
    s.color(TEXT)
        .font_size(UI)
        .class(ButtonClass, quiet_button)
        .class(TextInputClass, |s| {
            s.background(FIELD_BG)
                .border(0.0)
                .border_radius(CORNER_MD)
                .padding_horiz(SPACE_10)
                .padding_vert(SPACE_6)
                .outline_color(ACCENT_TINT)
                .set(CursorColor, INPUT_CURSOR)
                .hover(|s| s.background(FIELD_BG))
                .focus(|s| s.hover(|s| s.background(FIELD_BG)))
        })
        .class(PlaceholderTextClass, |s| s.color(PLACEHOLDER).font_size(UI))
        .class(CheckboxClass, |s| {
            s.background(FIELD_BG)
                .color(ACCENT)
                .border_color(RULE)
                .border_radius(CORNER_SM)
                .outline_color(ACCENT_TINT)
                .hover(|s| s.background(ROW_HOVER_BG))
                .focus(|s| s.border_color(RULE).hover(|s| s.background(ROW_HOVER_BG)))
                .active(|s| s.background(ROW_ACTIVE_BG))
        })
        .apply_custom(
            ScrollCustomStyle::new()
                .handle_background(RULE)
                .handle_thickness(SCROLL_HANDLE)
                .handle_rounded(true)
                .handle_border_radius(SCROLL_HANDLE / 2.0)
                .track_background(Color::TRANSPARENT),
        )
        .class(scroll::Handle, |s| {
            s.hover(|s| s.background(ROW_ACTIVE_BG))
                .active(|s| s.background(TEXT_MUTED))
        })
        .class(scroll::Track, |s| {
            s.hover(|s| s.background(Color::TRANSPARENT))
        })
        .class(EditorViewClass, |s| {
            s.set(SelectionColor, SELECTION).set(CursorColor, TEXT)
        })
}

/// Shared by both button styles: no border, and a keyboard focus ring that
/// suits the page.
fn button_base(s: Style) -> Style {
    s.border(0.0)
        .border_radius(CORNER_MD)
        .padding_horiz(SPACE_10)
        .padding_vert(SPACE_6)
        .outline_color(ACCENT_TINT)
}

/// The default button: no fill until hovered.
pub fn quiet_button(s: Style) -> Style {
    button_base(s)
        .background(Color::TRANSPARENT)
        .color(TEXT_SOFT)
        .hover(|s| s.background(ROW_HOVER_BG))
        .focus(|s| s.hover(|s| s.background(ROW_HOVER_BG)))
        .active(|s| s.background(ROW_ACTIVE_BG).color(TEXT))
        .disabled(|s| s.background(Color::TRANSPARENT).color(PLACEHOLDER))
}

/// The main action on a screen.
pub fn primary_button(s: Style) -> Style {
    button_base(s)
        .background(ACCENT)
        .color(ON_ACCENT)
        .font_weight(Weight::MEDIUM)
        .hover(|s| s.background(ACCENT_HOVER))
        .focus(|s| s.hover(|s| s.background(ACCENT_HOVER)))
        .active(|s| s.background(ACCENT_HOVER).color(ON_ACCENT))
}

// ── Fonts ───────────────────────────────────────────────────────────────────

/// Every face the editor uses. cosmic-text only picks a face from a named
/// family when the weight and style match exactly, and silently falls back to a
/// system font otherwise, so a style can only use what's listed here.
const FACES: &[&[u8]] = &[
    include_bytes!("../../../../assets/fonts/Newsreader-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/Newsreader-Italic.ttf"),
    include_bytes!("../../../../assets/fonts/Newsreader-Bold.ttf"),
    include_bytes!("../../../../assets/fonts/Newsreader-BoldItalic.ttf"),
    include_bytes!("../../../../assets/fonts/NewsreaderDisplay-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/NewsreaderDisplay-Italic.ttf"),
    include_bytes!("../../../../assets/fonts/NewsreaderDisplay-Bold.ttf"),
    include_bytes!("../../../../assets/fonts/NewsreaderDisplay-BoldItalic.ttf"),
    include_bytes!("../../../../assets/fonts/InstrumentSans-Regular.ttf"),
    include_bytes!("../../../../assets/fonts/InstrumentSans-Italic.ttf"),
    include_bytes!("../../../../assets/fonts/InstrumentSans-Medium.ttf"),
    include_bytes!("../../../../assets/fonts/InstrumentSans-SemiBold.ttf"),
    include_bytes!("../../../../assets/fonts/InstrumentSans-Bold.ttf"),
];

/// Load the bundled fonts and make Instrument Sans the default sans-serif, which
/// covers every label, input and `text_editor` that doesn't name a family.
///
/// Call once, before any view is built.
pub fn load_fonts() {
    let mut font_system = FONT_SYSTEM.lock();
    let db = font_system.db_mut();
    for face in FACES {
        db.load_font_data(face.to_vec());
    }
    db.set_sans_serif_family(SANS_FAMILY);
}

#[cfg(test)]
mod tests {
    use super::*;
    use floem::text::fontdb::{Database, Style, Weight};

    /// The family, weight and style each face must report, from the spec.
    const EXPECTED: &[(&str, u16, Style)] = &[
        (SERIF_FAMILY, 400, Style::Normal),
        (SERIF_FAMILY, 400, Style::Italic),
        (SERIF_FAMILY, 700, Style::Normal),
        (SERIF_FAMILY, 700, Style::Italic),
        (DISPLAY_FAMILY, 400, Style::Normal),
        (DISPLAY_FAMILY, 400, Style::Italic),
        (DISPLAY_FAMILY, 700, Style::Normal),
        (DISPLAY_FAMILY, 700, Style::Italic),
        (SANS_FAMILY, 400, Style::Normal),
        (SANS_FAMILY, 400, Style::Italic),
        (SANS_FAMILY, 500, Style::Normal),
        (SANS_FAMILY, 600, Style::Normal),
        (SANS_FAMILY, 700, Style::Normal),
    ];

    #[test]
    fn every_face_loads_with_the_exact_family_weight_and_style() {
        let mut db = Database::new();
        for face in FACES {
            db.load_font_data(face.to_vec());
        }
        assert_eq!(db.len(), EXPECTED.len());

        for (family, weight, style) in EXPECTED {
            let found = db.faces().any(|info| {
                info.families.iter().any(|(name, _)| name == family)
                    && info.weight == Weight(*weight)
                    && info.style == *style
            });
            assert!(found, "no face for {family} {weight} {style:?}");
        }
    }
}
