#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

//! Round-trip tests for the editor's `from_core` / `to_core` converters.
//!
//! Note: `lopress-core` does not promise byte-identical text round-trip for
//! arbitrary markdown — its proptest only asserts that
//! `serialize(parse(serialize(parse(x)))) == serialize(parse(x))`. So our
//! test contract is similarly *semantic*: a `Document` that goes through
//! `from_core → to_core` must equal the original `Document` under
//! `PartialEq` (modulo expected structural changes; see notes inline).
//! For `Opaque` blocks specifically we get byte-identity for free because
//! the original `Block` JSON is stashed verbatim inside the body.

use lopress_core::{parse, serialize, Block, Document, FrontMatter};
use lopress_editor::model::from_core::doc_from_core;
use lopress_editor::model::to_core::doc_to_core;
use lopress_editor::model::types::{
    BlockBody, BlockKind, EditorBlock, EditorDoc, InlineRun, ListItem, PluginMeta,
};
use lopress_plugin::PluginRegistry;
use serde_json::{json, Value};
use std::rc::Rc;

#[test]
fn paragraph_round_trips_via_document_equality() {
    let src = "---\ntitle: Test Post\n---\n# Heading 1\n\nA plain paragraph.\n\n## Heading 2\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn code_round_trips_with_language() {
    let src = "```rust\nfn main() {}\n```\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);

    // Sanity: the editor classifies it correctly.
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Code { lang } if &**lang == "rust"
    ));

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn opaque_block_preserved_byte_identical() {
    // Custom (`lopress:`) blocks are not yet plugin-aware (Task 17), so they
    // must travel through the editor as `Opaque` without any mutation. Because
    // we stash the original `Block` JSON in the body, byte-identity through
    // `serialize` is achievable here even though the supported subset only
    // promises semantic equality.
    let src =
        "before\n\n<!-- lopress:video {\"src\":\"a.mp4\"} -->\n<!-- /lopress:video -->\n\nafter\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &PluginRegistry::default());

    let video = &editor.blocks[1];
    assert!(
        matches!(&video.kind, BlockKind::Opaque { type_name } if type_name.as_ref() == "lopress:video"),
        "expected Opaque(lopress:video), got {:?}",
        video.kind
    );
    assert!(matches!(video.body, BlockBody::Opaque(_)));

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core, "Opaque body must reconstruct exact Block");
    assert_eq!(serialize(&core_back), serialize(&core));
}

#[test]
fn nested_block_inside_custom_falls_through_opaque() {
    let src =
        "<!-- lopress:callout {\"kind\":\"warning\"} -->\nbody para\n<!-- /lopress:callout -->\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &PluginRegistry::default());

    // A `lopress:callout` containing children must still come back as a single
    // Opaque block — the editor doesn't currently model nested children.
    assert_eq!(editor.blocks.len(), 1);
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "lopress:callout"
    ));

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn list_constructed_in_editor_round_trips_to_core_shape() {
    // Note: `lopress-core` currently drops list-item paragraph content during
    // its own parse cycle, so we can't drive this test from raw markdown.
    // Instead we build the editor representation directly, convert to core,
    // and assert the resulting `Document` matches the shape the rest of the
    // pipeline expects (list → list_item → paragraph).
    // A list block as `from_core` produces it: `BlockKind::List` body plus
    // `PluginMeta` claiming the native `list` type, so `to_core` serializes
    // it natively.
    let mut list_block = EditorBlock::list(
        false,
        vec![
            ListItem {
                id: Default::default(),
                runs: vec![InlineRun::plain("first item")],
            },
            ListItem {
                id: Default::default(),
                runs: vec![InlineRun::plain("second item")],
            },
        ],
    );
    let mut list_attrs = serde_json::Map::new();
    list_attrs.insert("ordered".to_string(), serde_json::Value::Bool(false));
    list_block.plugin = Some(PluginMeta {
        block_type_name: Rc::from("list"),
        attrs: list_attrs,
        attr_decls: Rc::from([]),
        builtin: true,
        editor: Some(Rc::from("list")),
        native: Some(Rc::from("list")),
    });
    let editor_doc = EditorDoc {
        front_matter: FrontMatter::default(),
        blocks: vec![list_block],
    };

    let core = doc_to_core(&editor_doc);
    assert_eq!(core.blocks.len(), 1);

    let list = &core.blocks[0];
    assert_eq!(list.r#type, "list");
    assert_eq!(list.attrs, json!({ "ordered": false }));
    assert_eq!(list.children.len(), 2);

    let item = &list.children[0];
    assert_eq!(item.r#type, "list_item");
    assert_eq!(item.children.len(), 1);
    assert_eq!(item.children[0].r#type, "paragraph");
    assert_eq!(item.children[0].text.as_deref(), Some("first item"));

    // And the reverse direction reconstructs the same editor structure.
    // The base list plugin must be registered for the native `list` type to
    // resolve; without it the block would degrade to `Opaque`.
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor_back = doc_from_core(&core, &registry);
    assert!(matches!(
        editor_back.blocks[0].kind,
        BlockKind::List { ordered: false }
    ));
    let BlockBody::List(items) = &editor_back.blocks[0].body else {
        panic!("expected List body, got {:?}", editor_back.blocks[0].body);
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].runs, vec![InlineRun::plain("first item")]);
    assert_eq!(items[1].runs, vec![InlineRun::plain("second item")]);
}

#[test]
fn nested_list_becomes_opaque() {
    // A list whose items contain anything beyond a single paragraph isn't
    // representable by the editor's flat `ListItem` model. It must fall
    // through to `Opaque` so the original tree is preserved verbatim.
    let nested_list = Block {
        r#type: "list".into(),
        attrs: json!({ "ordered": false }),
        children: vec![Block {
            r#type: "list_item".into(),
            attrs: json!({}),
            children: vec![
                Block::paragraph("top"),
                Block {
                    r#type: "list".into(),
                    attrs: json!({ "ordered": false }),
                    children: vec![Block {
                        r#type: "list_item".into(),
                        attrs: json!({}),
                        children: vec![Block::paragraph("nested")],
                        text: None,
                    }],
                    text: None,
                },
            ],
            text: None,
        }],
        text: None,
    };
    let core = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![nested_list],
    };

    let editor = doc_from_core(&core, &PluginRegistry::default());
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "list"
    ));

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core, "nested list must round-trip verbatim");
}

#[test]
fn empty_document_round_trips() {
    let src = "---\ntitle: Empty\n---\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &PluginRegistry::default());
    assert!(editor.blocks.is_empty());
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn front_matter_is_preserved() {
    let fm = FrontMatter {
        title: Some("Round Trip".into()),
        tags: vec!["a".into(), "b".into()],
        draft: true,
        ..Default::default()
    };

    let core = Document {
        front_matter: fm.clone(),
        blocks: vec![Block::paragraph("hello")],
    };
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);
    assert_eq!(editor.front_matter, fm);
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back.front_matter, fm);
}

#[test]
fn heading_levels_round_trip() {
    let src = "# h1\n\n## h2\n\n### h3\n\n#### h4\n\n##### h5\n\n###### h6\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);

    let levels: Vec<u8> = editor
        .blocks
        .iter()
        .map(|b| match b.kind {
            BlockKind::Heading(l) => l,
            _ => 0,
        })
        .collect();
    assert_eq!(levels, vec![1, 2, 3, 4, 5, 6]);

    assert_eq!(doc_to_core(&editor), core);
}

#[test]
fn code_block_carries_plugin_meta_after_from_core() {
    // A code block loaded from markdown must carry PluginMeta after the
    // migration to the native plugin path — proving the registry lookup
    // fires and native_code_from_core stamps the meta.
    let src = "```rust\nfn main() {}\n```\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);

    let block = &editor.blocks[0];
    assert!(
        block.plugin.is_some(),
        "loaded code block must carry PluginMeta"
    );
    let meta = block.plugin.as_ref().unwrap();
    assert_eq!(meta.block_type_name.as_ref(), "code");
    assert_eq!(meta.attrs.get("lang").and_then(Value::as_str), Some("rust"));
    assert!(meta.builtin);
    assert_eq!(meta.editor.as_deref(), Some("code"));
    assert_eq!(meta.native.as_deref(), Some("code"));
    assert!(matches!(&block.kind, BlockKind::Code { lang } if &**lang == "rust"));
    assert!(matches!(&block.body, BlockBody::Code(t) if t == "fn main() {}\n"));
}

#[test]
fn code_round_trip_via_native_path() {
    // After the from_core→to_core round-trip, the document must equal the
    // original — proving the native plugin path (not the removed hardcoded
    // arm) handles both directions.
    let src = "```python\nprint('hello')\n```\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn code_attrs_lang_mutation_serializes_correctly() {
    // Mutating plugin.attrs["lang"] before to_core must change the output —
    // proving native_block_to_core reads attrs (the source of truth), not
    // kind.lang.
    let src = "```rust\nfn main() {}\n```\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let mut editor = doc_from_core(&core, &registry);

    // Mutate the lang in attrs.
    if let Some(meta) = editor.blocks[0].plugin.as_mut() {
        meta.attrs
            .insert("lang".to_string(), Value::String("python".to_string()));
    }

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back.blocks[0].r#type, "code");
    assert_eq!(core_back.blocks[0].attrs, json!({ "lang": "python" }));
    assert_eq!(core_back.blocks[0].text.as_deref(), Some("fn main() {}\n"));
}

#[test]
fn pluginless_code_block_round_trips() {
    // Code blocks created at runtime via EditorBlock::code(...) have
    // plugin: None and serialize via the bottom-half BlockKind::Code arm
    // in block_to_core (retained as the fallback). This test proves the
    // round-trip still works for such blocks.
    let block = EditorBlock::code("go".into(), "package main\n".to_string());
    let doc = EditorDoc {
        front_matter: FrontMatter::default(),
        blocks: vec![block],
    };

    // Verify plugin-less.
    assert!(doc.blocks[0].plugin.is_none());

    let core = doc_to_core(&doc);
    assert_eq!(core.blocks[0].r#type, "code");
    assert_eq!(core.blocks[0].attrs, json!({ "lang": "go" }));
    assert_eq!(core.blocks[0].text.as_deref(), Some("package main\n"));

    // Round-trip back through from_core: without the registry the block
    // falls through to the catch-all and becomes Opaque — that's expected.
    // The important thing is to_core produces the right shape.
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor_back = doc_from_core(&core, &registry);
    // The code block now has PluginMeta (loaded through the registry path).
    assert!(
        editor_back.blocks[0].plugin.is_some(),
        "loaded code block must carry PluginMeta"
    );
    assert!(matches!(
        &editor_back.blocks[0].kind,
        BlockKind::Code { lang } if &**lang == "go"
    ));
}

// ============================================================================
// Unclassifiable block loading — no panics on disk data.
// ============================================================================

#[test]
fn unknown_block_type_loads_as_opaque_no_panic() {
    // A block type that is neither built-in nor in the registry must load
    // as Opaque without panicking. The body contains verbatim JSON so the
    // fallback view can render it.
    let unknown_block = Block {
        r#type: "unknown:foobar".into(),
        attrs: json!({ "foo": "bar" }),
        children: vec![],
        text: Some("raw text content".to_string()),
    };
    let core = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![unknown_block],
    };

    let editor = doc_from_core(&core, &PluginRegistry::default());
    assert_eq!(editor.blocks.len(), 1, "block must not be dropped");
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "unknown:foobar"
    ));
    assert!(matches!(
        &editor.blocks[0].body,
        BlockBody::Opaque(v) if v.get("text").and_then(Value::as_str) == Some("raw text content")
    ));
}

#[test]
fn malformed_attrs_loads_as_opaque_no_panic() {
    // A block with attrs that can't be parsed as an object should still
    // load without panicking — serde_json::to_value handles any Block.
    let malformed_block = Block {
        r#type: "weird:block".into(),
        attrs: json!("not-an-object"), // malformed: attrs should be an object
        children: vec![],
        text: None,
    };
    let core = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![malformed_block],
    };

    let editor = doc_from_core(&core, &PluginRegistry::default());
    assert_eq!(editor.blocks.len(), 1, "block must not be dropped");
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "weird:block"
    ));
}

#[test]
fn unregistered_plugin_type_loads_as_opaque() {
    // A block type matching a plugin namespace but not registered in the
    // current registry must load as Opaque, not panic.
    let custom_block = Block {
        r#type: "lopress:video".into(),
        attrs: json!({ "src": "video.mp4" }),
        children: vec![],
        text: None,
    };
    let core = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![custom_block],
    };

    // Use an empty registry — no plugins registered.
    let registry = PluginRegistry::default();
    let editor = doc_from_core(&core, &registry);
    assert_eq!(editor.blocks.len(), 1, "block must not be dropped");
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Opaque { type_name } if type_name.as_ref() == "lopress:video"
    ));

    // Round-trip: the Opaque body preserves the original JSON.
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn read_more_marker_survives_editor_round_trip() {
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let src = "before\n\n<!-- lopress:more -->\n<!-- /lopress:more -->\n\nafter\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &registry);
    let core_back = doc_to_core(&editor);
    let out = serialize(&core_back);
    assert_eq!(out, src);
}

#[test]
fn image_block_round_trips_with_caption() {
    let mut reg = PluginRegistry::default();
    reg.load_base_plugins().unwrap();
    let src = "![the alt](/images/p.jpg \"A caption\")\n";
    let core = lopress_core::parse(src).unwrap();
    let edoc = doc_from_core(&core, &reg);
    // The image becomes a BlockKind::Image with attrs in PluginMeta.
    assert_eq!(edoc.blocks.len(), 1);
    let back = doc_to_core(&edoc);
    assert_eq!(serialize(&back), src);
}

#[test]
fn template_form_block_round_trips_as_comment_container() {
    let src = "<!-- lopress:author-bio {\"name\":\"Jane\",\"bio\":\"Loves **Rust**\",\"spoiler\":true} -->\n<!-- /lopress:author-bio -->\n";
    let core = lopress_core::parse(src).unwrap();
    // The block should have type "lopress:author-bio" with attrs and no children.
    assert_eq!(core.blocks.len(), 1);
    let b = &core.blocks[0];
    assert_eq!(b.r#type, "lopress:author-bio");
    assert_eq!(b.attrs.get("name").and_then(|v| v.as_str()), Some("Jane"));
    assert_eq!(
        b.attrs.get("bio").and_then(|v| v.as_str()),
        Some("Loves **Rust**")
    );
    assert_eq!(b.attrs.get("spoiler").and_then(|v| v.as_bool()), Some(true));
    assert!(b.children.is_empty());
    // Round-trip: the Document must be structurally equal (JSON key order
    // may differ because serde_json::Map is a BTreeMap).
    let back = lopress_core::serialize(&core);
    let core_back = lopress_core::parse(&back).unwrap();
    assert_eq!(core_back, core);
}

#[test]
fn paragraph_round_trips_via_native_path() {
    // After migration, paragraph blocks must route through the native
    // registry path — not the hardcoded arm — proving the migration works.
    let src = "A plain paragraph.\n\nAnother one.\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);

    // Sanity: the editor classifies it correctly.
    for b in &editor.blocks {
        assert!(
            b.plugin.is_some(),
            "loaded paragraph must carry PluginMeta"
        );
        let meta = b.plugin.as_ref().unwrap();
        assert_eq!(meta.block_type_name.as_ref(), "paragraph");
        assert_eq!(meta.native.as_deref(), Some("paragraph"));
    }

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn heading_round_trips_via_native_path() {
    // After migration, heading blocks must route through the native registry
    // path — not the hardcoded arm.
    let src = "# h1\n\n## h2\n\n### h3\n";
    let core = parse(src).unwrap();
    let mut registry = PluginRegistry::default();
    registry.load_base_plugins().unwrap();
    let editor = doc_from_core(&core, &registry);

    for b in &editor.blocks {
        assert!(
            b.plugin.is_some(),
            "loaded heading must carry PluginMeta"
        );
        let meta = b.plugin.as_ref().unwrap();
        assert_eq!(meta.block_type_name.as_ref(), "heading");
        assert_eq!(meta.native.as_deref(), Some("heading"));
        assert!(meta.attrs.contains_key("level"));
    }

    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}
