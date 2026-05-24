Two small polish fixes in `crates/lopress-editor/src/ui/blocks/code_editor.rs`,
both noticed during GUI testing of the Stage 2 widget. Branch:
`feat/code-editor-block`. Commit each fix separately, conventional commit
style, `Co-Authored-By: Qwen <noreply@anthropic.com>` trailer.

## Fix 1: Last line of code body renders outside the frame

**Symptom.** A code block with N visual lines (N≥2) has its last line
visually clip below the grey frame's bottom border.

**Root cause.** In `editable_code_view` (`code_editor.rs:307-316`), the
`body_view` `stack` has both `.padding(10.)` and
`.height(lines * line_height)`. Floem's box model: padding subtracts from
the inner content area. So the editor view (which uses `size_full()` to
fill the inner area) gets a usable height of `lines * line_height - 20px`
— short by 20px (10 top + 10 bottom).

**Fix.** Account for the padding in the height calc. Change line 315
from:

```rust
.height(lines * line_height)
```

to:

```rust
.height(lines * line_height + 20.)
```

Or, equivalently, lift `padding(10.)` to a constant and use it both
places. Pick whichever you prefer; either is fine. Verify with a
multi-line code block in the GUI that the last line now sits cleanly
inside the frame.

Tests will pass either way (no test exercises this rendering path). The
verification is visual.

## Fix 2: No way to edit the code block's language

**Symptom.** The corner `rust` / `python` / `<lang>` label is visible but
read-only. No way to change a code block's language after creation.

**Root cause.** The lang field is stored in `block.plugin.attrs["lang"]`
(every code block has PluginMeta after the Stage 1 + ChangeType fixes —
verified via `grep "PluginMeta::code" crates/lopress-editor/src/actions.rs`),
and `apply_edit_attrs` in `actions.rs` already mirrors `attrs["lang"]`
into `BlockKind::Code.lang`. There's just no UI affordance to trigger an
`EditAttrs` action. The plugin attr form is suppressed for `builtin`
blocks (per spec, intentional), so the lang has no editable surface.

**Fix.** Make the corner lang label clickable/editable inline.

UX:
- Default state: render the lang label as today (right-corner grey text).
- On click: replace the label with a small `text_input` bound to a
  buffer signal initialised from the current lang.
- On blur (FocusLost): commit. If the buffer differs from the current
  lang, emit `BlockAction::EditAttrs { block_id, new_attrs }` where
  `new_attrs` is a fresh `Map` containing `"lang": Value::String(new)`.
  Then collapse back to label state.
- On Escape (if reasonable to wire): cancel without committing — collapse
  back to label, restore the original lang.

Implementation sketch (pi may revise — match Floem idioms in the rest of
the codebase):

```rust
// Inside editable_code_view, replace the existing lang_label block with:
let lang_sig: RwSignal<String> = RwSignal::new(lang.to_string());
let editing_lang: RwSignal<bool> = RwSignal::new(false);
let lang_on_action = on_action.clone();

let lang_widget = dyn_container(
    move || editing_lang.get(),
    move |is_editing| {
        if is_editing {
            let buf = RwSignal::new(lang_sig.get_untracked());
            let on_action = lang_on_action.clone();
            text_input(buf)
                .on_event_stop(EventListener::FocusLost, move |_| {
                    let new_lang = buf.get_untracked();
                    if new_lang != lang_sig.get_untracked() {
                        let mut new_attrs = Map::new();
                        new_attrs.insert(
                            "lang".to_string(),
                            Value::String(new_lang.clone()),
                        );
                        on_action(BlockAction::EditAttrs {
                            block_id,
                            new_attrs,
                        });
                        lang_sig.set(new_lang);
                    }
                    editing_lang.set(false);
                })
                .style(|s| {
                    s.font_size(11.)
                        .padding_horiz(8.)
                        .padding_vert(2.)
                        .min_width(60.)
                })
                .into_any()
        } else {
            label(move || lang_sig.get())
                .on_click_stop(move |_| editing_lang.set(true))
                .style(|s| {
                    s.color(Color::rgb8(120, 120, 120))
                        .font_size(11.)
                        .padding_horiz(8.)
                        .padding_vert(2.)
                        .cursor(floem::style::CursorStyle::Pointer)
                })
                .into_any()
        }
    },
);

let header = h_stack((empty().style(|s| s.flex_grow(1.0)), lang_widget));
```

Imports to add:
- `use crate::actions::BlockAction;` — already imported, check.
- `use floem::views::text_input;`
- `use floem::event::EventListener;`
- `use serde_json::{Map, Value};`

**Caveat.** The `text_input` should ideally auto-focus when it appears
(so the user can immediately type). Floem's `text_input` usually picks
up focus on first paint when it's the only focusable widget in its
container — verify in the GUI. If it doesn't, request focus explicitly
via the editor view id pattern used elsewhere in the codebase. If it's
not obvious how, leave it without auto-focus and flag as a follow-up.

**Test it manually:**
1. Open a doc, create a code block, click its `rust` corner label.
2. It should become a text input. Type `python`, click outside.
3. The corner label should now read `python`.
4. The model: `/state` (via the debug ctrl server) should show
   `kind: "Code", lang: "python"`.
5. Save and reopen — lang should persist.

## Run after each commit

- `cargo test --workspace`
- `cargo check --workspace`

## Report

When done:
- Two commit hashes.
- Whether auto-focus on the text_input worked (Fix 2).
- Anything that didn't behave like the sketch suggested.
