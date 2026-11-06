# Paper theme: implementation plan

**Spec:** `docs/superpowers/specs/2026-11-06-paper-theme-design.md`
**Branch:** `feature/paper-theme`, from `main`. One PR.

**Approach:** land the fonts and tokens first, with nothing visibly restyled except
the UI font. Then move the app across one area at a time. A half-converted app looks
mixed but works, so every step could ship.

The stray colour test starts with an allowlist of the 27 files that still hold colour
literals. Each step removes the files it finished, and a file can't slip back. Step 5
empties the list and deletes it.

Every step ends with `bash scripts/check.sh` passing, after touching the changed `.rs`
files so clippy re-lints. Live checks run on a `lopress new` scratch site only. Save
screenshots under `.pi/results/2026-11-06-paper-theme/`, and open each one.

**Pixel proof:** where a step names colours, sample them from a `/screenshot` PNG with
Pillow in a scratchpad venv, and compare them to the token. Sample flat areas, away
from text.

## Step 1: Fonts and tokens

**Depends on:** nothing.

**Changes:**

- New `scripts/build-fonts.py`, in the style of `scripts/build-icon.py`: downloads the
  variable fonts at the commits in the spec, writes the 13 static faces, and sets names,
  weight classes and style bits. Newsreader Display's Regular and Italic are weight 500
  marked 400, at optical size 32. Newsreader is at optical size 16. Instrument Sans is at
  width 100.
- New `assets/fonts/`: the 13 `.ttf` files, `OFL-Newsreader.txt`,
  `OFL-InstrumentSans.txt`.
- New `ui/theme.rs`:
  - The colour, text, spacing and corner tokens from the spec. Tokens not used yet need
    no `#[allow(dead_code)]` if they're `pub`.
  - `FACES`, a table of family, weight, style and `include_bytes!` data.
  - `pub fn load_fonts()`, which loads `FACES` and calls
    `set_sans_serif_family("Instrument Sans")`.
- `ui/mod.rs`: `pub mod theme;`
- `lib.rs`: call `ui::theme::load_fonts()` at the top of `run()`, before any view.
- New `tests/theme_tests.rs`: walks `src/ui`, and fails on `Color::rgb`, `Color::rgba`,
  `Color::WHITE`, `Color::BLACK`, `rgb8(` or `rgba8(` in any file other than
  `ui/theme.rs` and an `ALLOWED` list. It also fails if a file in `ALLOWED` no longer has
  a literal, so the list has to shrink as work lands. `Color::TRANSPARENT` is fine.
- `.claude/skills/building-on-floem/SKILL.md`: add the exact-face rule and the fact that
  the block editor ignores `Styling::weight`.

**Proof:** a test in `theme.rs` loads `FACES` into a fresh `fontdb::Database` and finds
every family, weight and style in the spec's table. `cargo test -p lopress-editor
--test theme_tests` passes with all 27 files in `ALLOWED`. Live: a `/screenshot` shows
sidebar and inspector text in Instrument Sans.

## Step 2: Global styling

**Depends on:** step 1.

**Changes:**

- `ui/theme.rs`: `pub fn root_style(s: Style) -> Style` with the classes in the spec:
  `ButtonClass`, `TextInputClass`, `PlaceholderTextClass`, `scroll::Handle`,
  `scroll::Track`, `CheckboxClass`, `EditorViewClass`. Add `primary_button` and
  `quiet_button`.
- `ui/mod.rs`: apply `root_style` in the `.style` on the outermost `dyn_container` in
  `root_view`, so the welcome screen gets it too.

If a view's own style beats a class, leave that for the step that converts the view.

**Proof:** pixel proof: an inspector input fills with `FIELD_BG`, on hover a footer
button fills with `ROW_HOVER_BG`. A screenshot shows a thin scrollbar and a selection in
the `SELECTION` colour, with caret and placeholder visible.

## Step 3: Article text

**Depends on:** step 1.

**Changes:**

- `ui/theme.rs`: `TextRole` (size, family, line height), with `BODY`, `H1` to `H6` and
  `CODE`.
- `ui/blocks/style_span.rs`: `InlineRunStyling` holds a `TextRole`, and implements
  `font_size`, `font_family` and `line_height` from it. Try a smaller size for code runs.
- `ui/blocks/inline_editor.rs`: `build_block_editor` takes a `TextRole`.
- `ui/blocks/paragraph.rs`, `heading.rs`, `list.rs`, `table.rs`, `code_editor.rs`: pass
  the right role. Remove `BODY_FONT_SIZE`, `font_size_for` and `CODE_FONT_SIZE`. Read-only
  runs in `paragraph.rs` use the same roles and the serif family.
- `ui/editor_pane.rs`: `PAGE_BG`, the 620 text measure, top padding 56.
- `ui/blocks/mod.rs`: focused block fills `BLOCK_FOCUS_BG` with corner 6. Remove the
  transparent border, and keep the block's position with padding.
- `ALLOWED`: remove `paragraph.rs`, `style_span.rs`, `inline_editor.rs`, `list.rs`,
  `editor_pane.rs`, `blocks/mod.rs`.

**Seam:** `table.rs` and `code_editor.rs` only change their role here. Step 5 does their
colours.

**Proof:** the theme test passes with the shorter list. Live, on a post with every
heading level, bold, italic, bold italic, inline code, a list and a table:

- Pixel proof: the column is `PAGE_BG`, and a focused block is `BLOCK_FOCUS_BG`.
- Measure from a screenshot: at a window wide enough for the full column, the text runs
  620 wide, within a few pixels.
- Focusing a block doesn't move its text: compare screenshots before and after a click.
- No block balloons or clips after typing past a line wrap, and Enter, Backspace merge,
  Shift+Enter and Ctrl+S with the caret kept all still work, checked with `/state` and
  the file on disk.

## Step 4: Chrome

**Depends on:** steps 2 and 3.

**Changes:**

- New `ui/icons.rs`: plus, sliders and chevron as inline SVG strings, using floem's `svg`.
- New `ui/toggle.rs`: the switch from the spec, with the API
  `toggle(get, on_change)`.
- `ui/sidebar.rs`, `ui/inspector.rs`, `ui/footer.rs`, `ui/notes_drawer.rs`,
  `ui/disk_bar.rs`, `ui/link_bar.rs`, `ui/toolbar.rs`: as in the spec's tables. The
  inspector's Draft checkbox becomes the toggle.
- `ui/blocks/mod.rs`: re-check `TOOLBAR_HEIGHT_PX` against the new toolbar height. It
  must stay in-flow.
- `ALLOWED`: remove those seven files.

**Proof:** the theme test passes with the shorter list. Pixel proof: the sidebar and
inspector are `CHROME_BG`, the New post button is `ACCENT`, the open row is
`ROW_ACTIVE_BG`, and the toolbar is `SURFACE`. Live:

- Every toolbar button works by `/click`, checked with `/state`.
- Clicking the toggle flips `draft`, which shows in the file after Ctrl+S.
- The notes drawer opens and closes, and its dot appears when the post has notes.
- The disk bar shows after an outside edit, and Refresh works.
- The link bar opens and applies a link.

## Step 5: Blocks, menus and dialogs

**Depends on:** step 4.

**Changes:**

- `ui/blocks/table.rs`, `code_editor.rs`, `plugin.rs`, `image.rs`, `fallback.rs`,
  `opaque.rs`, `separator.rs`, `read_more.rs`: as in the spec's table. Plugin checkboxes
  become the toggle.
- `ui/dnd.rs`, `ui/slash_menu.rs`, `ui/nav_editor.rs`, `ui/confirm_dialog.rs`: as in the
  spec's table.
- `ui/mod.rs`: the site settings modal and any remaining literals.
- `ui/welcome.rs`: the mockup's layout.
- `tests/theme_tests.rs`: `ALLOWED` is empty, so delete it and the check that goes
  with it.

**Proof:** the theme test passes with no allowlist. `grep -rnE 'Color::(rgb|rgba)|rgba?8\('
crates/lopress-editor/src/ui --include=*.rs` lists only `ui/theme.rs`. Live, a screenshot
of each: every block type, the slash menu, the confirm dialog, the site settings modal,
the nav editor, and the welcome screen. Plugin toggles save, drag and drop still moves a
block, and each dialog button still works.

## Step 6: Final pass and docs

**Depends on:** step 5.

**Changes:**

- `docs/architecture.md`: add `ui/theme.rs`, `ui/icons.rs`, `ui/toggle.rs` and the font
  loading.
- `README.md`: a line crediting the fonts and their licence.
- `.claude/skills/building-on-floem/SKILL.md`: note that colours come from `ui/theme.rs`.
- Anything the live pass finds.

**Proof:** the spec's whole verification list, on a fresh scratch site, against the
mockups. A release build (`cargo build --release`) runs from a directory with no
`assets/`, and still shows the bundled fonts. The gate on the final commit.

## Not doing

- Dark mode, an accent picker, site theme changes or any behaviour change, as in the spec.
- Testing on macOS or Linux. CI builds and runs tests there, but nobody looks at the
  result, so the PR says so.
- Migrating to floem main. It's next, and needs its own spec.
