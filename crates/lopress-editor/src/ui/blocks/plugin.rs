//! Plugin block rendering (Path 1: built-in editor + attr form).
//!
//! A plugin block stacks three layers:
//!   1. Header strip — `block.plugin.block_type_name` styled as a tag, so
//!      the user can see at a glance which plugin owns the block.
//!   2. Attr form — one input per `attr_decls` entry, dispatched by `ui`
//!      hint (`text` / `select` / `checkbox` / `number`). Edits emit
//!      `BlockAction::EditAttrs` with the full updated attrs map.
//!   3. Body editor — dispatched on `block.kind` / `block.body` to one of
//!      the built-in editors (paragraph / heading / code / list). Read-only
//!      for now for code/list bodies; paragraph and heading get the same
//!      editable widget the rest of the editor uses.

use crate::actions::BlockAction;
use crate::model::types::{BlockBody, BlockId, BlockKind, EditorBlock, EditorDoc};
use crate::ui::blocks::inline_editor::{ActionSink, FocusPublisher};
use crate::ui::blocks::{code, heading, list, paragraph};
use crate::ui::dnd::DndState;
use floem::peniko::Color;
use floem::reactive::{RwSignal, SignalGet, SignalUpdate, SignalWith};
use floem::text::Weight;
use floem::views::{
    checkbox, h_stack_from_iter, label, text_input, v_stack, v_stack_from_iter, Decorators,
};
use floem::{AnyView, IntoView};
use lopress_plugin::{AttrDecl, AttrType};
use serde_json::Value;
use std::rc::Rc;

const HEADER_BG: Color = Color::rgb8(238, 234, 250);
const HEADER_FG: Color = Color::rgb8(80, 60, 130);
const FORM_BG: Color = Color::rgb8(250, 250, 252);
const BORDER: Color = Color::rgb8(220, 215, 235);

/// Build the full plugin block view.
#[allow(clippy::too_many_arguments)]
pub fn plugin_block_view(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    _dnd: DndState,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
    let block_id = block.id;
    let Some(meta) = block.plugin.clone() else {
        // Shouldn't be called for non-plugin blocks; render a placeholder.
        return label(|| "(missing plugin meta)".to_string()).into_any();
    };

    let body = render_body(
        block,
        on_action.clone(),
        focus_target,
        focus_pub,
        current_doc,
        on_undo,
        on_redo,
    );

    // Builtin (base-plugin) blocks suppress plugin chrome: no header strip,
    // no attr form — they render as plain editable blocks.
    if meta.builtin {
        return v_stack((body,)).style(|s| s.width_full()).into_any();
    }

    let header = label({
        let name = meta.block_type_name.clone();
        move || name.clone()
    })
    .style(|s| {
        s.padding_horiz(8.)
            .padding_vert(2.)
            .background(HEADER_BG)
            .color(HEADER_FG)
            .font_size(11.)
            .font_weight(Weight::SEMIBOLD)
            .border_radius(3.)
    });

    let attrs_sig: RwSignal<serde_json::Map<String, Value>> = RwSignal::new(meta.attrs.clone());
    let on_action_for_attrs = on_action.clone();
    let form = build_attr_form(&meta.attr_decls, attrs_sig, block_id, on_action_for_attrs);

    v_stack((header, form, body))
        .style(|s| {
            s.gap(4.)
                .padding(6.)
                .border(1.)
                .border_color(BORDER)
                .border_radius(4.)
                .background(FORM_BG)
                .width_full()
        })
        .into_any()
}

fn build_attr_form(
    decls: &[AttrDecl],
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
) -> AnyView {
    if decls.is_empty() {
        return floem::views::empty().into_any();
    }
    // We keep field names in attr_decls' iteration order; the public
    // `attr_decls` snapshot is built from the plugin manifest's BTreeMap so
    // it's already alphabetical.
    let mut rows: Vec<AnyView> = Vec::with_capacity(decls.len());
    for decl in decls {
        // Each decl needs its own field name. The current AttrDecl from
        // lopress-plugin doesn't carry the key alongside the value when we
        // collect into a Vec — so we infer name from the attrs map order.
        // Prefer explicit naming via the future schema work; for now we use
        // the field's `ui` hint and key-by-position.
        let _ = decl;
    }
    // Render rows by iterating attrs by current keys (snapshot once); each
    // decl is matched by index. This is workable for the first version: the
    // attrs map and decl list are both in alphabetical order at load time.
    let snapshot = attrs_sig.get_untracked();
    let names: Vec<String> = snapshot.keys().cloned().collect();
    for (i, decl) in decls.iter().enumerate() {
        let name = names.get(i).cloned().unwrap_or_else(|| format!("attr_{i}"));
        rows.push(attr_row(
            name,
            decl.clone(),
            attrs_sig,
            block_id,
            on_action.clone(),
        ));
    }
    v_stack_from_iter(rows)
        .style(|s| s.gap(2.).padding_horiz(2.))
        .into_any()
}

fn attr_row(
    name: String,
    decl: AttrDecl,
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
) -> AnyView {
    let lbl_text = name.clone();
    let lbl = label(move || lbl_text.clone()).style(|s| {
        s.min_width(80.)
            .color(Color::rgb8(110, 100, 130))
            .font_size(12.)
    });

    let ui_hint = decl.ui.as_deref().unwrap_or("text").to_string();
    let input: AnyView = match (decl.kind, ui_hint.as_str()) {
        (AttrType::Bool, _) | (_, "checkbox") => {
            attr_checkbox(name.clone(), attrs_sig, block_id, on_action.clone()).into_any()
        }
        (_, "select") => attr_select(
            name.clone(),
            decl.options.clone(),
            attrs_sig,
            block_id,
            on_action.clone(),
        )
        .into_any(),
        (AttrType::Number, _) | (_, "number") => {
            attr_text(name.clone(), attrs_sig, block_id, on_action.clone(), true).into_any()
        }
        _ => attr_text(name.clone(), attrs_sig, block_id, on_action.clone(), false).into_any(),
    };

    h_stack_from_iter(vec![lbl.into_any(), input])
        .style(|s| s.gap(8.).items_center())
        .into_any()
}

fn attr_text(
    name: String,
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
    numeric: bool,
) -> impl IntoView {
    let initial: String = attrs_sig.with_untracked(|m| {
        m.get(&name)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                _ => v.to_string(),
            })
            .unwrap_or_default()
    });
    let buf: RwSignal<String> = RwSignal::new(initial);
    let name_for_commit = name.clone();
    let attrs_for_commit = attrs_sig;
    let on_action_for_commit = on_action;
    text_input(buf)
        .on_event(floem::event::EventListener::FocusLost, move |_| {
            let s = buf.get_untracked();
            let parsed = if numeric {
                s.parse::<f64>()
                    .map(|n| {
                        serde_json::Number::from_f64(n)
                            .map(Value::Number)
                            .unwrap_or(Value::String(s.clone()))
                    })
                    .unwrap_or(Value::String(s.clone()))
            } else {
                Value::String(s)
            };
            attrs_for_commit.update(|m| {
                m.insert(name_for_commit.clone(), parsed);
            });
            let new_attrs = attrs_for_commit.get_untracked();
            on_action_for_commit(BlockAction::EditAttrs {
                block_id,
                new_attrs,
            });
            floem::event::EventPropagation::Continue
        })
        .style(|s| s.font_size(12.).padding_horiz(4.).min_width(160.))
}

fn attr_checkbox(
    name: String,
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
) -> impl IntoView {
    let checked: RwSignal<bool> = RwSignal::new(
        attrs_sig.with_untracked(|m| m.get(&name).and_then(Value::as_bool).unwrap_or(false)),
    );
    checkbox(move || checked.get()).on_click_stop(move |_| {
        let new_value = !checked.get_untracked();
        checked.set(new_value);
        attrs_sig.update(|m| {
            m.insert(name.clone(), Value::Bool(new_value));
        });
        let new_attrs = attrs_sig.get_untracked();
        on_action(BlockAction::EditAttrs {
            block_id,
            new_attrs,
        });
    })
}

fn attr_select(
    name: String,
    options: Vec<String>,
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
) -> impl IntoView {
    // No stock dropdown in Floem 0.2 — render a row of small toggle buttons.
    // The currently-selected option highlights.
    let opts = options.clone();
    let selected: RwSignal<Option<String>> = RwSignal::new(
        attrs_sig.with_untracked(|m| m.get(&name).and_then(|v| v.as_str().map(str::to_string))),
    );
    let mut buttons: Vec<AnyView> = Vec::with_capacity(opts.len().max(1));
    if opts.is_empty() {
        // No options declared: fall back to a free-text field.
        return attr_text(name, attrs_sig, block_id, on_action, false).into_any();
    }
    for opt in opts {
        let opt_for_btn = opt.clone();
        let opt_for_label = opt.clone();
        let name_for_btn = name.clone();
        let on_action_for_btn = on_action.clone();
        let btn = floem::views::button(label(move || opt_for_label.clone()))
            .action(move || {
                selected.set(Some(opt_for_btn.clone()));
                attrs_sig.update(|m| {
                    m.insert(name_for_btn.clone(), Value::String(opt_for_btn.clone()));
                });
                let new_attrs = attrs_sig.get_untracked();
                on_action_for_btn(BlockAction::EditAttrs {
                    block_id,
                    new_attrs,
                });
            })
            .style(move |s| {
                let s = s.font_size(11.).padding_horiz(6.).padding_vert(1.);
                if selected.get().as_deref() == Some(opt.as_str()) {
                    s.background(Color::rgb8(210, 220, 240))
                        .font_weight(Weight::SEMIBOLD)
                } else {
                    s
                }
            });
        buttons.push(btn.into_any());
    }
    h_stack_from_iter(buttons).style(|s| s.gap(2.)).into_any()
}

fn render_body(
    block: &EditorBlock,
    on_action: ActionSink,
    focus_target: RwSignal<Option<BlockId>>,
    focus_pub: FocusPublisher,
    current_doc: RwSignal<Option<EditorDoc>>,
    on_undo: Rc<dyn Fn()>,
    on_redo: Rc<dyn Fn()>,
) -> AnyView {
    use crate::ui::blocks::editor_registry::{editor_for, EditorContext};

    // Registry path: a manifest `editor` key with a registered widget wins.
    if let Some(key) = block.plugin.as_ref().and_then(|m| m.editor.as_deref()) {
        if let Some(widget) = editor_for(key) {
            let ctx = EditorContext {
                block,
                on_action: on_action.clone(),
                focus_target,
                focus_pub,
                current_doc,
                on_undo: Rc::clone(&on_undo),
                on_redo: Rc::clone(&on_redo),
            };
            return widget(&ctx);
        }
    }

    // Fallback: editor keys not yet migrated to the registry (paragraph,
    // heading, code) still dispatch on the Rust `BlockKind` enum.
    let block_id = block.id;
    match (&block.kind, &block.body) {
        (BlockKind::Paragraph, BlockBody::Inline(runs)) => paragraph::render_paragraph_editable(
            runs,
            block_id,
            on_action,
            focus_target,
            focus_pub,
            current_doc,
            on_undo,
            on_redo,
        )
        .into_any(),
        (BlockKind::Heading(level), BlockBody::Inline(runs)) => heading::render_heading_editable(
            *level,
            runs,
            block_id,
            on_action,
            focus_target,
            focus_pub,
            current_doc,
            on_undo,
            on_redo,
        )
        .into_any(),
        (BlockKind::Code { lang }, BlockBody::Code(text)) => {
            code::render_code(lang, text).into_any()
        }
        (BlockKind::List { ordered }, BlockBody::List(items)) => list::editable_list_view(
            items,
            block_id,
            *ordered,
            on_action,
            focus_target,
            focus_pub,
            current_doc,
        ),
        _ => floem::views::empty().into_any(),
    }
}
