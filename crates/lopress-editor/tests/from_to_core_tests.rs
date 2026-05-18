#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::field_reassign_with_default
)]

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
use serde_json::json;

#[test]
fn paragraph_round_trips_via_document_equality() {
    let src = "---\ntitle: Test Post\n---\n# Heading 1\n\nA plain paragraph.\n\n## Heading 2\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &PluginRegistry::default());
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back, core);
}

#[test]
fn code_block_round_trips_with_language() {
    let src = "```rust\nfn main() {}\n```\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &PluginRegistry::default());

    // Sanity: the editor classifies it correctly.
    assert!(matches!(
        &editor.blocks[0].kind,
        BlockKind::Code { lang } if lang == "rust"
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
        matches!(&video.kind, BlockKind::Opaque { type_name } if type_name == "lopress:video"),
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
        BlockKind::Opaque { type_name } if type_name == "lopress:callout"
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
        block_type_name: "list".to_string(),
        attrs: list_attrs,
        attr_decls: vec![],
        builtin: true,
        editor: Some("list".to_string()),
        native: Some("list".to_string()),
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
        BlockKind::Opaque { type_name } if type_name == "list"
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
    let mut fm = FrontMatter::default();
    fm.title = Some("Round Trip".into());
    fm.tags = vec!["a".into(), "b".into()];
    fm.draft = true;

    let core = Document {
        front_matter: fm.clone(),
        blocks: vec![Block::paragraph("hello")],
    };
    let editor = doc_from_core(&core, &PluginRegistry::default());
    assert_eq!(editor.front_matter, fm);
    let core_back = doc_to_core(&editor);
    assert_eq!(core_back.front_matter, fm);
}

#[test]
fn heading_levels_round_trip() {
    let src = "# h1\n\n## h2\n\n### h3\n\n#### h4\n\n##### h5\n\n###### h6\n";
    let core = parse(src).unwrap();
    let editor = doc_from_core(&core, &PluginRegistry::default());

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
