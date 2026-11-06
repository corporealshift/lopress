# Paper theme

**Status:** design approved, implementation plan in `docs/superpowers/plans/2026-11-06-paper-theme.md`
**Date:** 2026-11-06
**Mockups:** direction A in https://claude.ai/artifact/HApQyqv8Wu7K2LywR2ncwV
(`Main.dc.html` for the editor, `PaperWelcome.dc.html` for the welcome screen)

## Problem

The editor works but looks unstyled. It uses floem's default controls and system
fonts, and its colours are 114 literals spread across 28 files, so there is no
single place to change the look.

We want the Paper look from the mockups, and one theme that everything reads from.
That theme also helps the floem migration: peniko jumps from 0.2 to 0.6 there, so
colour code will probably need changing, and it should only need changing once.

## What it does

- A new `ui/theme.rs` holds every colour, text size, spacing value and corner radius.
- Newsreader sets the article text and Instrument Sans sets the UI. Both are bundled
  in the binary.
- One root style restyles every button, text input, checkbox and scrollbar.
- Every screen moves to the Paper look, including the ones the mockup doesn't show.
- Draft and plugin checkboxes become a toggle switch.
- The sidebar buttons and the notes arrow get simple line icons.

### Out of scope

- Dark mode. Tokens are constants, because that's the simplest thing that works.
  Making them switchable later is a contained change.
- An accent picker. The accent is brick red.
- Site themes. This changes the editor only, not published sites.
- Editor behaviour. Nothing about how editing works changes.

## Fonts

### What the test build showed

A throwaway build on floem 0.2.0, on Windows, confirmed all of this in the running
editor:

- Fonts load with `FONT_SYSTEM.lock().db_mut().load_font_data(bytes)`.
- Both families render in block editors, labels, text inputs and the notes editor.
- Typing and wrapping still work with the new font and line height.
- `db_mut().set_sans_serif_family("Instrument Sans")` makes it the default for all
  text, including `text_editor`, so no view needs a family set for UI text.
- `box_shadow_*` renders a soft shadow on the toolbar.

### Exact faces only

cosmic-text 0.12 only picks a face from a named family when the weight and style
match exactly (`font_weight_diff == 0` in `font/fallback/mod.rs`). Anything else
silently falls through to a system font. In the test, a heading set to a family with
only a 500 face rendered in Segoe UI, and bold italic did the same until its face
was added.

So every combination we use needs its own file:

| Family | Faces | Used for |
|---|---|---|
| Newsreader | Regular, Italic, Bold, Bold Italic | Paragraphs, lists, tables. Optical size 16 |
| Newsreader Display | Regular, Italic, Bold, Bold Italic | Headings, welcome title. Optical size 32 |
| Instrument Sans | Regular, Italic, Medium, SemiBold, Bold | All UI text |

That's 13 static TTFs, about 1.5 MB. They live in `assets/fonts/` and are compiled in
with `include_bytes!`.

### Heading weight

The block editor ignores `Styling::weight`: `Editor::new_text_layout` builds its
`Attrs` without it, so every block's default weight is 400. Headings need weight 500.

So the Regular and Italic faces of Newsreader Display are really weight 500, and are
marked 400 in the file. Bold stays 700, so bold runs in a heading still work. This is
a small lie in the font metadata, kept in one script, and it means no styling code
has to fight the editor. It worked in the test build.

### Producing the files

`scripts/build-fonts.py` makes the static faces from the variable fonts in
`google/fonts` using `fontTools.varLib.instancer`, and sets their names and weight
classes. It records the upstream commits, and the generated files are committed, so
a normal build needs no Python. This matches `scripts/build-icon.py`.

Upstream commits:

- `ofl/newsreader`: `8b0a1d0f5983c89bc2b93f1b5fb55f9e252744b5`
- `ofl/instrumentsans`: `0b58fb370093f9a9f4ff785d94405710b79de67c`

Both are under the SIL Open Font License with no Reserved Font Name, so modified
versions can keep their names. The licences sit beside the fonts as
`OFL-Newsreader.txt` and `OFL-InstrumentSans.txt`, and the fonts keep their copyright
and licence name records, which is how the licence travels inside the binary.

### Inline code

Monospace stays the system font. In the test build a line containing inline code sat
about 2px off even spacing, because the fallback font has different metrics. Try
setting code runs slightly smaller. If that doesn't fix it, accept the 2px.

## Theme

`ui/theme.rs` holds constants, grouped as below. The values come from the mockup,
except the ones marked new, which the mockup doesn't cover.

### Colours

| Token | Value | Used for |
|---|---|---|
| `CHROME_BG` | `#EFEAE0` | Sidebar, inspector, footer |
| `PAGE_BG` | `#FBF8F2` | Article column, welcome |
| `DRAWER_BG` | `#F5F0E6` | Notes drawer |
| `FIELD_BG` | `#F8F4EB` | Text inputs |
| `SURFACE` | `#FFFFFF` | Toolbar, menus, dialogs |
| `BLOCK_FOCUS_BG` | `#F3EDE1` | Focused block |
| `ROW_ACTIVE_BG` | `#E2DACA` | Open document in the sidebar |
| `ROW_HOVER_BG` | `#E8E1D3` | Hovered rows and quiet buttons (new) |
| `TEXT` | `#26221D` | Main text |
| `TEXT_SOFT` | `#4F483F` | Quiet buttons, toolbar |
| `TEXT_MUTED` | `#6B6358` | Labels, footer |
| `PLACEHOLDER` | `#6F665A` | Placeholders |
| `RULE` | `#DDD5C7` | Drawer and footer rules |
| `RULE_SOFT` | `#E6DFD2` | Toolbar dividers, list rules |
| `ACCENT` | `#A63A2A` | Primary button, Draft, active toggles, delete |
| `ACCENT_HOVER` | `#7E2A1D` | Primary button hover, links |
| `ACCENT_TINT` | `#F3E4DF` | Pressed toolbar button, disk bar |
| `OK` | `#4C7A4F` | Build status dot |
| `WARN` | `#8A5A12` | Unsaved (new) |
| `SELECTION` | `ACCENT` at 20% | Text selection (new) |
| `SHADOW` | `rgba(60,45,25,.16)` | Toolbar, menus, dialogs |

Errors use `ACCENT`, because Paper has one accent colour.

### Text

| Token | Size | Family |
|---|---|---|
| `BODY` | 19, line height 1.6 | Newsreader |
| `H1` | 40, line height 1.15 | Newsreader Display |
| `H2` | 25, line height 1.25 | Newsreader Display |
| `H3` to `H6` | 21, 19, 17, 15, line height 1.3 | Newsreader Display |
| `UI` | 13 | Instrument Sans |
| `UI_SMALL` | 12 | Instrument Sans |

The mockup's 19px body replaces today's 15px. `BODY_FONT_SIZE` and the heading sizes
are replaced by a `TextRole` (size, family, line height) that `build_block_editor`
takes in place of a bare font size, and `InlineRunStyling` implements `font_family`
and `line_height` from it. Anything that renders runs read-only uses the same role.

### Spacing and corners

Spacing tokens cover the values the mockup uses: 2, 4, 6, 8, 10, 12, 16, 18, 24.
Corners are 5 (toolbar buttons), 6 (controls, rows) and 8 (toolbar, menus, dialogs).

### Why constants

`Color::rgb8` is `const`, so tokens are plain `pub const`. There's no theme struct to
thread through views, and no signal. Each token keeps a name that says what it's for.

## Global styling

floem's default theme is applied per class, and a parent's style can override it.
`theme::root_style(s)` on the root view sets classes for:

- `ButtonClass`: borderless, `ROW_HOVER_BG` on hover
- `TextInputClass` and `PlaceholderTextClass`: `FIELD_BG`, no border, `PLACEHOLDER`
- `scroll::Handle` and `scroll::Track`: thin, tan handle, no track
- `CheckboxClass`
- `EditorViewClass`: `SELECTION` and the caret colour

Two named helpers in `theme.rs`, `primary_button` and `quiet_button`, cover the
variants the mockup uses.

## Screens

### From the mockup

- **Layout:** the sidebar and inspector stay 220 and 280 wide. They're on `CHROME_BG`
  with no dividing borders, and the article column sits on `PAGE_BG`.
- **Article column:** the text measures 620 wide, centred. The column's `max_width`
  includes its padding and the drag handle gutter, so it's larger than 620. Top padding
  is 56.
- **Sidebar:** group labels 12 semibold, rows 6 corners, `ROW_ACTIVE_BG` on the open
  document. The Draft pill becomes small red text. New post is a filled accent button,
  and New page and Site settings are quiet buttons with icons.
- **Focused block:** `BLOCK_FOCUS_BG` with a 6 corner, no border. Today's transparent
  1px border goes, with padding keeping everything in place, so focus doesn't shift the
  text.
- **Toolbar:** `SURFACE`, corner 8, shadow, no border. Buttons are 26 tall and
  borderless, the active type uses `ACCENT_TINT` and `ACCENT`, and dividers use
  `RULE_SOFT`. It keeps its in-flow slot (`TOOLBAR_HEIGHT_PX`), because absolute
  overflow is click-dead in floem 0.2.
- **Inspector:** labels above borderless `FIELD_BG` inputs, 32 tall.
- **Notes drawer:** a 34 tall `DRAWER_BG` bar with an arrow, plus a small accent dot
  when the post has notes.
- **Footer:** `CHROME_BG` with a top `RULE`, a coloured status dot beside the build
  text, then save state, word count, and the preview URL on the right.
- **Welcome:** the mockup's layout, with a 52px title, a primary Open button, and
  recents as a ruled list.

The Save button came after the mockup. It's a quiet button in the footer, next to the
save state.

### Not in the mockup

These use the same tokens, and judgement:

| Screen | Treatment |
|---|---|
| Dialogs (confirm, site settings, nav editor) | `SURFACE`, corner 8, shadow, over a light scrim |
| Slash menu | `SURFACE`, corner 8, shadow, `ROW_HOVER_BG` rows |
| Link bar | `DRAWER_BG` bar with a top `RULE` |
| Disk bar | `ACCENT_TINT` bar |
| Tables | `RULE_SOFT` cell rules, no outer box |
| Code blocks | `DRAWER_BG`, corner 6, system monospace |
| Plugin blocks | `DRAWER_BG` card, label in `UI_SMALL`, inputs as in the inspector |
| Image, fallback and opaque blocks | `DRAWER_BG` card, `TEXT_MUTED` labels |
| Separator, read more | `RULE` line, `TEXT_MUTED` label |
| Drag and drop | `ACCENT` drop line |

### Toggle and icons

`ui/toggle.rs` is a 34 by 20 switch, `ACCENT` when on, `RULE` when off. It replaces the
Draft checkbox and the plugin attribute checkboxes.

Icons use floem's `svg` view with a small set of inline strings in `ui/icons.rs`: plus,
sliders and chevron. They take their colour from `theme.rs`.

## Failure behaviour

| Failure | Result |
|---|---|
| A font file is corrupt | fontdb skips it, and the text falls back to a system font. Caught before shipping by the face test |
| A style asks for a face we don't bundle | Silent system font, as above. Weights in use are limited to the table |
| A glyph isn't in the font | System fallback, as for any font |

Nothing here can stop the app starting.

## Rejected alternatives

| Alternative | Why not |
|---|---|
| System fonts, or a stack of likely fonts | Loses the look, and differs between machines |
| Variable font files | The exact-face rule needs one face per weight. Static files went straight in, so variable files weren't tested |
| Set weight 500 on heading lines through `apply_attr_styles` | Every bold run would then need its own handling. Baking it into the family needs no code |
| Theming through styles, leaving the literals in place | No single place for colours, so the migration wouldn't get any help |
| A runtime theme struct or signal | Nothing needs to change at runtime without dark mode |
| Bundling a monospace font | More files for a 2px problem that a smaller size may fix |
| Migrating to floem main first | Agreed order is Paper first. The theme makes the migration smaller, and only font loading should need redoing |

## Verification

- **Face test, in `theme.rs`:** loads every bundled font into a fresh `fontdb::Database`
  and asserts each family, weight and style in the table exists. This is the guard
  against silent fallback.
- **Stray colour test, `tests/theme_tests.rs`:** scans `src/ui` and fails on a colour
  literal outside `ui/theme.rs`.
- **Live, on a scratch site:** screenshots of every screen in both tables, compared
  with the mockups, on Windows only. Typing, selection, wrapping, block focus,
  toolbar clicks, drag and drop, dialogs and the toggle still work.
- **Gate:** `bash scripts/check.sh`.

macOS and Linux get the same fonts by the same path, but aren't checked here.
