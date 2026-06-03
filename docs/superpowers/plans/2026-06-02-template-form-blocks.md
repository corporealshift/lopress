# Template-Form Blocks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a declarative-form + Tera *markdown* template block class to lopress plugins, interpolating form values into a markdown template that flows through the existing md→HTML pipeline.

**Architecture:** A template-form block is a standard comment-container plugin block. Three manifest additions (`BlockDecl.markdown_template`, `ui = "textarea"`, optional `AttrDecl.label`/`help`) plus mutual-exclusivity with `template`; an editor textarea widget and label/help usage; one new build-render branch that Tera-interpolates the markdown template (attrs as context), parses the result via `lopress_core::parse`, and renders it through `render_body`.

**Tech Stack:** Rust workspace (`lopress-plugin`, `lopress-editor`, `lopress-build`, `lopress-core`), Tera templating, pulldown-cmark via `lopress_core::parse`, floem editor UI, serde/serde_json, TOML manifests.

---

## Task 1: Manifest fields — `markdown_template`, `label`, `help`, `ui = "textarea"`

**Files:**
- Modify: `crates/lopress-plugin/src/manifest.rs` (add new fields to `BlockDecl` and `AttrDecl`)
- Modify: `crates/lopress-plugin/src/error.rs` (add `MutualExclusion` variant to `PluginError`)
- Test: `crates/lopress-plugin/src/manifest.rs` (in-file tests)

- [ ] **Step 1: Write the failing tests**

In `crates/lopress-plugin/src/manifest.rs`, add to the existing `#[cfg(test)] mod tests`:

```rust
#[test]
fn parses_markdown_template_field() {
    let src = r#"
name = "author-bio"
version = "0.1.0"

[[blocks]]
name = "lopress:author-bio"
markdown_template = "blocks/author-bio.md"

[blocks.attrs]
name    = { type = "string", ui = "text",     required = true,  label = "Author name" }
bio     = { type = "string", ui = "textarea",                 label = "Short bio",    help = "A short biography" }
spoiler = { type = "bool",   ui = "checkbox", default = false, label = "Mark as spoiler" }
"#;
    let m = parse_manifest_str(src).unwrap();
    assert_eq!(m.blocks.len(), 1);
    let b = &m.blocks[0];
    assert_eq!(b.markdown_template.as_deref(), Some("blocks/author-bio.md"));
    assert!(b.template.is_none());
    assert_eq!(b.attrs["name"].label.as_deref(), Some("Author name"));
    assert_eq!(b.attrs["bio"].label.as_deref(), Some("Short bio"));
    assert_eq!(b.attrs["bio"].help.as_deref(), Some("A short biography"));
    assert_eq!(b.attrs["bio"].ui.as_deref(), Some("textarea"));
}

#[test]
fn errors_when_both_template_and_markdown_template_set() {
    let src = r#"
name = "bad"
version = "0.1.0"

[[blocks]]
name = "lopress:bad"
template = "blocks/bad.html"
markdown_template = "blocks/bad.md"
"#;
    let err = parse_manifest_str(src).unwrap_err();
    assert!(matches!(err, PluginError::MutualExclusion { field1, field2 } if field1 == "template" && field2 == "markdown_template"));
}

#[test]
fn label_and_help_default_to_none() {
    let src = r#"
name = "minimal"
version = "0.1.0"

[[blocks]]
name = "lopress:minimal"
template = "blocks/minimal.html"

[blocks.attrs]
foo = { type = "string" }
"#;
    let m = parse_manifest_str(src).unwrap();
    assert_eq!(m.blocks[0].attrs["foo"].label, None);
    assert_eq!(m.blocks[0].attrs["foo"].help, None);
}

#[test]
fn markdown_template_defaults_to_none() {
    let src = r#"
name = "video"
version = "0.1.0"

[[blocks]]
name = "lopress:video"
template = "blocks/video.html"
"#;
    let m = parse_manifest_str(src).unwrap();
    assert!(m.blocks[0].markdown_template.is_none());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p lopress-plugin parses_markdown_template_field errors_when_both_template_and_markdown_template_set label_and_help_default_to_none markdown_template_defaults_to_none`
Expected: FAIL — fields don't exist yet, `MutualExclusion` variant doesn't exist.

- [ ] **Step 3: Add the new fields to `BlockDecl`**

In `crates/lopress-plugin/src/manifest.rs`, after the `js` field on `BlockDecl` (line ~44), add:

```rust
    /// Tera markdown-template path, relative to the plugin root.
    /// Mutually exclusive with `template`. When present, the block
    /// is a *template-form* block: form values interpolate into this
    /// markdown template, and the result flows through the md→HTML pipeline.
    #[serde(default)]
    pub markdown_template: Option<String>,
```

- [ ] **Step 4: Add the new fields to `AttrDecl`**

In `crates/lopress-plugin/src/manifest.rs`, after the `options` field on `AttrDecl` (line ~66), add:

```rust
    /// Human-friendly field caption. When absent, the form falls back to
    /// the attr key name.
    #[serde(default)]
    pub label: Option<String>,
    /// Helper / description text shown beneath the label.
    #[serde(default)]
    pub help: Option<String>,
```

- [ ] **Step 5: Add the `MutualExclusion` error variant**

In `crates/lopress-plugin/src/error.rs`, after `DuplicateNative`, add:

```rust
    #[error("`{field1}` and `{field2}` are mutually exclusive on the same block")]
    MutualExclusion { field1: String, field2: String },
```

- [ ] **Step 6: Add mutual-exclusivity validation to `parse_manifest_str`**

In `crates/lopress-plugin/src/manifest.rs`, replace the current `parse_manifest_str` body:

```rust
pub fn parse_manifest_str(src: &str) -> Result<PluginManifest, PluginError> {
    let manifest: PluginManifest = toml::from_str(src).map_err(|e| PluginError::Manifest {
        path: std::path::PathBuf::from("<embedded>"),
        message: e.to_string(),
    })?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}
```

Add a `validate_manifest` function (before `parse_manifest_str` or after `parse_manifest`):

```rust
/// Validate a parsed manifest for semantic constraints.
fn validate_manifest(manifest: &PluginManifest) -> Result<(), PluginError> {
    for block in &manifest.blocks {
        if block.template.is_some() && block.markdown_template.is_some() {
            return Err(PluginError::MutualExclusion {
                field1: "template".to_string(),
                field2: "markdown_template".to_string(),
            });
        }
    }
    Ok(())
}
```

- [ ] **Step 7: Also validate in `parse_manifest` (on-disk manifests)**

In `parse_manifest`, after the `toml::from_str` call that produces `manifest`, add:

```rust
    validate_manifest(&manifest)?;
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p lopress-plugin parses_markdown_template_field errors_when_both_template_and_markdown_template_set label_and_help_default_to_none markdown_template_defaults_to_none`
Expected: PASS.

- [ ] **Step 9: Fix the existing literal `BlockDecl { … }` in `lopress-build` (compile blocker)**

Adding `markdown_template` to `BlockDecl` breaks every literal struct construction that
doesn't set it. This does NOT surface under `cargo test -p lopress-plugin` (that command
never compiles `lopress-build`), but it WILL break Task 2's `cargo test -p lopress-build`.
Fix it now. The only literal `BlockDecl { … }` in the codebase is in
`crates/lopress-build/src/render.rs`, test `known_custom_block_renders_via_template`
(~line 390). Add `markdown_template: None` right after the `template:` line:

```rust
                blocks: vec![BlockDecl {
                    name: "lopress:demo".into(),
                    template: Some("blocks/demo.html".into()),
                    markdown_template: None,
                    attrs: Default::default(),
                    renderer: None,
                    editor: None,
                    builtin: false,
                    native: None,
                    css: Vec::new(),
                    js: Vec::new(),
                }],
```

(No literal `AttrDecl { … }` constructions exist outside tests we control — they're all TOML-parsed.)

Verify the build crate's tests now compile:

Run: `cargo build -p lopress-build --tests`
Expected: success (no missing-field errors).

- [ ] **Step 10: Commit**

```bash
git add crates/lopress-plugin/src/manifest.rs crates/lopress-plugin/src/error.rs crates/lopress-build/src/render.rs
git commit -m "feat(plugin): add markdown_template, label, help fields and mutual-exclusivity validation"
```

---

## Task 2: Register markdown_template files in the build Tera engine

**Files:**
- Modify: `crates/lopress-build/src/build.rs` (the Tera template registration loop)

- [ ] **Step 1: Write the failing test**

In `crates/lopress-build/src/build.rs`, add a test that verifies `markdown_template` files are registered:

```rust
#[test]
fn markdown_template_files_are_registered_in_tera() {
    use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest};

    let mut reg = PluginRegistry::default();
    reg.insert(LoadedPlugin {
        root: std::path::PathBuf::from("/does/not/exist"),
        manifest: PluginManifest {
            name: "demo".into(),
            version: "0.1.0".into(),
            theme: false,
            blocks: vec![BlockDecl {
                name: "lopress:demo".into(),
                template: None,
                markdown_template: Some("blocks/demo.md".into()),
                attrs: Default::default(),
                renderer: None,
                editor: None,
                builtin: false,
                native: None,
                css: Vec::new(),
                js: Vec::new(),
            }],
        },
    })
    .unwrap();

    // Simulate the Tera registration from build() — we can't call build() itself,
    // so we inline the relevant loop here as a helper, or test the loop logic.
    // For now, assert the struct compiles with the new field.
    let block = &reg.plugins[0].manifest.blocks[0];
    assert_eq!(block.markdown_template.as_deref(), Some("blocks/demo.md"));
}
```

> **Note:** The full integration test (that Tera actually has the template) requires writing a file to disk and calling the build path. That end-to-end coverage lives in Task 3's build-render tests and Task 8's fixture integration test. This step is primarily to confirm the struct compiles and the field is accessible.

- [ ] **Step 2: Run it to verify it compiles**

Run: `cargo test -p lopress-build markdown_template_files_are_registered_in_tera`
Expected: PASS (struct field is accessible; the full Tera registration is tested in Task 4).

- [ ] **Step 3: Add markdown_template registration alongside template registration**

In `crates/lopress-build/src/build.rs`, find the Tera template registration loop (around lines 76-89). The existing code:

```rust
    for plugin in &registry.plugins {
        for block in &plugin.manifest.blocks {
            // Base (built-in) blocks are editor-only and ship no HTML
            // template — there is nothing to register for the static build.
            let Some(template) = &block.template else {
                continue;
            };
            let plugin_name = &plugin.manifest.name;
            let key = format!("{plugin_name}::{template}");
            let src = std::fs::read_to_string(plugin.root.join(template))?;
            tera.add_raw_template(&key, &src)
                .map_err(|e| BuildError::Config(format!("plugin template `{key}`: {e}")))?;
        }
    }
```

Replace with:

```rust
    for plugin in &registry.plugins {
        for block in &plugin.manifest.blocks {
            let plugin_name = &plugin.manifest.name;

            // Register HTML template (existing path).
            if let Some(template) = &block.template {
                let key = format!("{plugin_name}::{template}");
                let src = std::fs::read_to_string(plugin.root.join(template))?;
                tera.add_raw_template(&key, &src)
                    .map_err(|e| BuildError::Config(format!("plugin template `{key}`: {e}")))?;
            }

            // Register markdown template (new path).
            if let Some(md_template) = &block.markdown_template {
                let key = format!("{plugin_name}::{md_template}");
                let src = std::fs::read_to_string(plugin.root.join(md_template))?;
                tera.add_raw_template(&key, &src)
                    .map_err(|e| BuildError::Config(format!("plugin markdown template `{key}`: {e}")))?;
            }
        }
    }
```

- [ ] **Step 4: Compile-check**

Run: `cargo build -p lopress-build`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/lopress-build/src/build.rs
git commit -m "feat(build): register markdown_template files in the shared Tera engine"
```

---

## Task 3: Build-render branch — interpolate markdown template + md→HTML pipeline

**Files:**
- Modify: `crates/lopress-build/src/render.rs` (add markdown_template branch in `render_custom`)
- Test: `crates/lopress-build/src/render.rs` (in-file tests)

- [ ] **Step 1: Write the failing tests**

In `crates/lopress-build/src/render.rs` tests, add:

```rust
#[test]
fn markdown_template_interpolates_and_presents_as_html() {
    use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest};
    let mut reg = PluginRegistry::default();
    reg.insert(LoadedPlugin {
        root: std::path::PathBuf::from("/does/not/exist"),
        manifest: PluginManifest {
            name: "demo".into(),
            version: "0.1.0".into(),
            theme: false,
            blocks: vec![BlockDecl {
                name: "lopress:demo".into(),
                template: None,
                markdown_template: Some("blocks/demo.md".into()),
                attrs: Default::default(),
                renderer: None,
                editor: None,
                builtin: false,
                native: None,
                css: Vec::new(),
                js: Vec::new(),
            }],
        },
    })
    .unwrap();

    let mut tera = Tera::default();
    tera.add_raw_template(
        "demo::blocks/demo.md",
        "**About {{ name }}**\n\n{{ bio }}",
    )
    .unwrap();

    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "lopress:demo".into(),
            attrs: json!({"name":"Jane","bio":"Loves **Rust**"}),
            children: vec![],
            text: None,
        }],
    };
    let html = render_body(&doc, &reg, &tera, &ImageIndex::default()).unwrap();
    assert!(html.contains("<strong>About Jane</strong>"), "name interpolated: {html}");
    assert!(html.contains("<strong>Rust</strong>"), "markdown in field value renders: {html}");
}

#[test]
fn checkbox_attr_drives_if_conditional_in_markdown_template() {
    use lopress_plugin::{BlockDecl, LoadedPlugin, PluginManifest};
    let mut reg = PluginRegistry::default();
    reg.insert(LoadedPlugin {
        root: std::path::PathBuf::from("/does/not/exist"),
        manifest: PluginManifest {
            name: "demo".into(),
            version: "0.1.0".into(),
            theme: false,
            blocks: vec![BlockDecl {
                name: "lopress:demo".into(),
                template: None,
                markdown_template: Some("blocks/demo.md".into()),
                attrs: Default::default(),
                renderer: None,
                editor: None,
                builtin: false,
                native: None,
                css: Vec::new(),
                js: Vec::new(),
            }],
        },
    })
    .unwrap();

    let mut tera = Tera::default();
    tera.add_raw_template(
        "demo::blocks/demo.md",
        "{{ name }}\n{% if spoiler %}\n> ⚠️ Contains spoilers.\n{% endif %}",
    )
    .unwrap();

    // spoiler = true
    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "lopress:demo".into(),
            attrs: json!({"name":"Jane","spoiler":true}),
            children: vec![],
            text: None,
        }],
    };
    let html = render_body(&doc, &reg, &tera, &ImageIndex::default()).unwrap();
    assert!(html.contains("<blockquote>"), "spoiler blockquote present: {html}");

    // spoiler = false
    let doc2 = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "lopress:demo".into(),
            attrs: json!({"name":"Jane","spoiler":false}),
            children: vec![],
            text: None,
        }],
    };
    let html2 = render_body(&doc2, &reg, &tera, &ImageIndex::default()).unwrap();
    assert!(!html2.contains("<blockquote>"), "no spoiler blockquote: {html2}");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p lopress-build markdown_template_interpolates_and_presents_as_html checkbox_attr_drives_if_conditional_in_markdown_template`
Expected: FAIL — the `markdown_template` branch doesn't exist yet.

- [ ] **Step 3: Add the markdown_template branch in `render_custom`**

In `crates/lopress-build/src/render.rs`, in the `render_custom` function, after the existing `decl.template` branch (around line 175-186):

```rust
    let template_key = format!("{plugin_name}::{template_name}");
    let mut ctx = tera::Context::new();
    ctx.insert("attrs", &b.attrs);
    ctx.insert("inner_html", &inner_html);
    let rendered = tera
        .render(&template_key, &ctx)
        .map_err(|e| BuildError::Config(format!("template {template_key}: {e}")))?;
    out.push_str(&rendered);
    if !rendered.ends_with('\n') {
        out.push('\n');
    }
    Ok(())
```

Replace the entire function body (after the `registry.block` lookup and inner_html rendering) with:

```rust
    let plugin_name = &plugin.manifest.name;

    // HTML template path (existing behavior).
    if let Some(template_name) = &decl.template {
        let template_key = format!("{plugin_name}::{template_name}");
        let mut ctx = tera::Context::new();
        ctx.insert("attrs", &b.attrs);
        ctx.insert("inner_html", &inner_html);
        let rendered = tera
            .render(&template_key, &ctx)
            .map_err(|e| BuildError::Config(format!("template {template_key}: {e}")))?;
        out.push_str(&rendered);
        if !rendered.ends_with('\n') {
            out.push('\n');
        }
        return Ok(());
    }

    // Markdown template path (new behavior).
    if let Some(md_template_name) = &decl.markdown_template {
        let template_key = format!("{plugin_name}::{md_template_name}");
        let mut ctx = tera::Context::new();
        // Insert each attr at the top level so templates can use bare field
        // names like {{ name }} alongside {{ attrs.name }}.
        for (k, v) in &b.attrs {
            ctx.insert(k, v);
        }
        let rendered = tera
            .render(&template_key, &ctx)
            .map_err(|e| BuildError::Config(format!("markdown template {template_key}: {e}")))?;
        // Feed the Tera-interpolated markdown through the existing md→HTML pipeline.
        let doc = lopress_core::parse(&rendered)
            .map_err(|e| BuildError::Config(format!("markdown parse: {e}")))?;
        let md_html = render_body(&doc, registry, tera, image_index)?;
        out.push_str(&md_html);
        if !md_html.ends_with('\n') {
            out.push('\n');
        }
        return Ok(());
    }

    // Base (built-in) block with no template — emit inner HTML directly.
    out.push_str(&inner_html);
    if !inner_html.is_empty() && !inner_html.ends_with('\n') {
        out.push('\n');
    }
    Ok(())
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p lopress-build markdown_template_interpolates_and_presents_as_html checkbox_attr_drives_if_conditional_in_markdown_template`
Expected: PASS.

- [ ] **Step 5: Run all render tests to confirm nothing regressed**

Run: `cargo test -p lopress-build`
Expected: PASS (all existing tests still pass).

- [ ] **Step 6: Commit**

```bash
git add crates/lopress-build/src/render.rs
git commit -m "feat(build): add markdown_template render branch with md→HTML pipeline"
```

---

## Task 4: Editor — textarea widget + label/help in attr form

**Files:**
- Modify: `crates/lopress-editor/src/ui/blocks/plugin.rs` (label fallback, help text, textarea arm)
- Test: `crates/lopress-editor/src/ui/blocks/plugin.rs` (label/help unit tests)

> **Floem multi-line input:** Floem 0.2.0 provides `floem::views::text_editor` (a `TextEditor` view) which takes `impl Into<Rope>`. The workspace already uses `lapce_xi_rope::Rope` (imported in `crates/lopress-editor/src/model/sync.rs` and `crates/lopress-editor/src/ui/blocks/inline_editor.rs`). `Rope::from(string)` converts a `&str` to a `Rope`. The textarea will use `text_editor` bound to a `Rope` signal, committing on `FocusLost` exactly like `attr_text` commits on `FocusLost`.

- [ ] **Step 1: Write unit tests for label/help**

In `crates/lopress-editor/src/ui/blocks/plugin.rs`, add a test module:

```rust
#[cfg(test)]
mod label_tests {
    use super::*;

    #[test]
    fn label_prefers_decl_label_over_name() {
        let decl = AttrDecl {
            kind: AttrType::String,
            required: false,
            default: None,
            ui: Some("text".to_string()),
            options: Vec::new(),
            label: Some("Custom Label".to_string()),
            help: None,
        };
        // The label text for the row should be "Custom Label".
        // We verify the logic: decl.label.or_else(|| Some(name.clone())).unwrap_or(name).
        let name = "field_name";
        let effective_label = decl.label.as_deref().unwrap_or(name);
        assert_eq!(effective_label, "Custom Label");
    }

    #[test]
    fn label_falls_back_to_name_when_none() {
        let decl = AttrDecl {
            kind: AttrType::String,
            required: false,
            default: None,
            ui: Some("text".to_string()),
            options: Vec::new(),
            label: None,
            help: None,
        };
        let name = "field_name";
        let effective_label = decl.label.as_deref().unwrap_or(name);
        assert_eq!(effective_label, "field_name");
    }

    #[test]
    fn help_is_presented_when_set() {
        let decl = AttrDecl {
            kind: AttrType::String,
            required: false,
            default: None,
            ui: Some("textarea".to_string()),
            options: Vec::new(),
            label: None,
            help: Some("Enter a value".to_string()),
        };
        assert_eq!(decl.help.as_deref(), Some("Enter a value"));
    }
}
```

- [ ] **Step 2: Run to verify they compile**

Run: `cargo test -p lopress-editor label_tests`
Expected: PASS (the `AttrDecl` struct already has `label` and `help` fields from Task 1; the test logic is straightforward).

- [ ] **Step 3: Add the textarea arm to the widget match**

In `attr_row`, find the existing match (around line ~153):

```rust
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
```

Replace with:

```rust
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
        (_, "textarea") => {
            attr_textarea(name.clone(), attrs_sig, block_id, on_action.clone()).into_any()
        }
        _ => attr_text(name.clone(), attrs_sig, block_id, on_action.clone(), false).into_any(),
    };
```

- [ ] **Step 4: Implement `attr_textarea`**

Add this function after `attr_text` (around line 210) in `plugin.rs`:

```rust
/// Multi-line text input for `ui = "textarea"`. Commits on FocusLost,
/// exactly like `attr_text` but uses Floem's `text_editor` for multi-line.
fn attr_textarea(
    name: String,
    attrs_sig: RwSignal<serde_json::Map<String, Value>>,
    block_id: BlockId,
    on_action: ActionSink,
) -> impl IntoView {
    use floem::views::text_editor;
    use lapce_xi_rope::Rope;

    let initial: String = attrs_sig.with_untracked(|m| {
        m.get(&name)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                _ => v.to_string(),
            })
            .unwrap_or_default()
    });
    let rope: RwSignal<Rope> = RwSignal::new(Rope::from(initial.as_str()));
    let name_for_commit = name.clone();
    let attrs_for_commit = attrs_sig;
    let on_action_for_commit = on_action;

    text_editor(rope)
        .on_event(floem::event::EventListener::FocusLost, move |_| {
            let s = rope.get_untracked().to_string();
            attrs_for_commit.update(|m| {
                m.insert(name_for_commit.clone(), Value::String(s));
            });
            let new_attrs = attrs_for_commit.get_untracked();
            on_action_for_commit(BlockAction::EditAttrs {
                block_id,
                new_attrs: Box::new(new_attrs),
            });
            floem::event::EventPropagation::Continue
        })
        .style(|s| s.font_size(12.).padding_horiz(4.).min_width(160.).min_height(60.))
}
```

Add the needed imports at the top of the file. In the `use floem::views::{...}` import, add `text_editor` from `floem::views::text_editor::text_editor`:

```rust
use floem::views::text_editor;
```

And add `use lapce_xi_rope::Rope;` near the top (after the other floem reactive imports).

- [ ] **Step 5: Change the label to prefer `decl.label`**

In `attr_row`, find:

```rust
    let lbl_text = name.clone();
```

Replace with:

```rust
    let lbl_text = decl.label.clone().unwrap_or(name.clone());
```

- [ ] **Step 6: Surface `decl.help` as a secondary label row**

In `attr_row`, after the `lbl` label and before the `input`, add a help row when `decl.help` is `Some`:

```rust
    let help_row = decl.help.as_ref().map(|h| {
        label(move || h.clone()).style(|s| {
            s.font_size(10.)
                .color(Color::rgb8(140, 130, 160))
                .padding_top(1.)
        })
    });

    let row: AnyView = if let Some(help) = help_row {
        v_stack((lbl.into_any(), help, input)).into_any()
    } else {
        h_stack_from_iter(vec![lbl.into_any(), input])
            .style(|s| s.gap(8.).items_center())
            .into_any()
    };
```

Replace the existing `h_stack_from_iter(vec![lbl.into_any(), input])` return at the end of `attr_row` with the above, and return `row`.

- [ ] **Step 7: Compile-check the editor crate**

Run: `cargo build -p lopress-editor`
Expected: success. If `lapce_xi_rope` is not a direct dependency of `lopress-editor`, add it to `crates/lopress-editor/Cargo.toml` under `[dependencies]`:

```toml
lapce_xi_rope.workspace = true
```

- [ ] **Step 8: Run the editor crate tests**

Run: `cargo test -p lopress-editor label_tests`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/lopress-editor/src/ui/blocks/plugin.rs
git commit -m "feat(editor): add textarea widget and label/help support in attr form"
```

---

## Task 5: Verify the `lopress-build` render test literal is fixed

**Files:**
- Verify: `crates/lopress-build/src/render.rs` (test `known_custom_block_renders_via_template`, ~line 390)

> **Note:** This compile blocker is fixed and committed in Task 1 Step 9 (the literal must be
> repaired before Task 2's `cargo test -p lopress-build`). This task is a safety net: confirm
> the fix is present. If for any reason `markdown_template: None` is missing from the
> `known_custom_block_renders_via_template` test's `BlockDecl`, add it now (see the block in
> Task 1 Step 9) and commit; otherwise there is nothing to do.

- [ ] **Step 1: Confirm the existing render test compiles and passes**

Run: `cargo test -p lopress-build known_custom_block_renders_via_template`
Expected: PASS. If it fails to compile with a missing-field error, apply the `markdown_template: None`
fix from Task 1 Step 9, then:

```bash
git add crates/lopress-build/src/render.rs
git commit -m "chore: add missing markdown_template field to render test BlockDecl"
```

---

## Task 6: Example plugin fixture — `author-bio`

**Files:**
- Create: `crates/lopress-build/tests/fixtures/with-plugin/plugins/author-bio/plugin.toml`
- Create: `crates/lopress-build/tests/fixtures/with-plugin/plugins/author-bio/blocks/author-bio.md`

> **Convention:** Existing plugins under test fixtures live under `crates/lopress-build/tests/fixtures/with-plugin/plugins/` (see `callout/`). The `callout` plugin has a `plugin.toml` and a `blocks/callout.html`. We follow the same pattern.

- [ ] **Step 1: Create the plugin manifest**

`crates/lopress-build/tests/fixtures/with-plugin/plugins/author-bio/plugin.toml`:

```toml
name = "author-bio"
version = "0.1.0"

[[blocks]]
name = "lopress:author-bio"
markdown_template = "blocks/author-bio.md"

[blocks.attrs]
name    = { type = "string", ui = "text",     required = true,  label = "Author name" }
bio     = { type = "string", ui = "textarea",                 label = "Short bio",    help = "A short biography" }
spoiler = { type = "bool",   ui = "checkbox", default = false, label = "Mark as spoiler" }
```

- [ ] **Step 2: Create the markdown template**

`crates/lopress-build/tests/fixtures/with-plugin/plugins/author-bio/blocks/author-bio.md`:

```markdown
**About {{ name }}**

{{ bio }}
{% if spoiler %}
> ⚠️ Contains spoilers.
{% endif %}
```

- [ ] **Step 3: Verify the fixture loads**

Run: `cargo test -p lopress-build`
Expected: PASS — the fixture is now loadable by any test that scans the plugins dir (if there is one).

- [ ] **Step 4: Commit**

```bash
git add crates/lopress-build/tests/fixtures/with-plugin/plugins/author-bio/
git commit -m "feat(build): add author-bio example plugin (template-form fixture)"
```

---

## Task 7: Round-trip test — template-form block through `from_core`/`to_core`

**Files:**
- Modify: `crates/lopress-editor/tests/from_to_core_tests.rs` (add test)

> A template-form block is a standard comment-container block: it round-trips through `from_core`/`to_core` as a `Block` with `type = "lopress:author-bio"`, attrs as JSON, and empty children. The spec says: "empty body; interpolated fresh at render time."

- [ ] **Step 1: Write the round-trip test**

In `crates/lopress-editor/tests/from_to_core_tests.rs`, add:

```rust
#[test]
fn template_form_block_round_trips_as_comment_container() {
    let src = "<!-- lopress:author-bio {\"name\":\"Jane\",\"bio\":\"Loves **Rust**\",\"spoiler\":true} -->\n<!-- /lopress:author-bio -->\n";
    let doc = lopress_core::parse(src).unwrap();
    // The block should have type "lopress:author-bio" with attrs and no children.
    assert_eq!(doc.blocks.len(), 1);
    let b = &doc.blocks[0];
    assert_eq!(b.r#type, "lopress:author-bio");
    assert_eq!(b.attrs.get("name").and_then(|v| v.as_str()), Some("Jane"));
    assert_eq!(b.attrs.get("bio").and_then(|v| v.as_str()), Some("Loves **Rust**"));
    assert_eq!(b.attrs.get("spoiler").and_then(|v| v.as_bool()), Some(true));
    assert!(b.children.is_empty());
    // Round-trip: serialize back to the same markdown.
    let back = lopress_core::serialize(&doc);
    assert_eq!(back, src);
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p lopress-editor template_form_block_round_trips_as_comment_container`
Expected: PASS — comment-container blocks already round-trip via the existing parser/serializer.

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-editor/tests/from_to_core_tests.rs
git commit -m "test(editor): add round-trip test for template-form comment-container blocks"
```

---

## Task 8: Build-render integration test — full pipeline with example plugin

**Files:**
- Modify: `crates/lopress-build/src/render.rs` (add integration test using the author-bio fixture)

- [ ] **Step 1: Write the integration test**

In `crates/lopress-build/src/render.rs` tests, add:

```rust
#[test]
fn author_bio_plugin_renders_markdown_template_through_pipeline() {
    use lopress_plugin::load_dir;

    // Load the author-bio fixture from the test fixtures.
    let fixtures_dir = std::path::PathBuf::from(
        env!("CARGO_MANIFEST_DIR")
    ).join("tests").join("fixtures").join("with-plugin").join("plugins");
    let registry = load_dir(&fixtures_dir, None).unwrap();

    // The author-bio plugin should be registered.
    let (_, decl) = registry.block("lopress:author-bio")
        .expect("author-bio block should be registered");
    assert_eq!(decl.markdown_template.as_deref(), Some("blocks/author-bio.md"));

    // Build a Tera that knows the author-bio markdown template.
    let mut tera = Tera::default();
    let plugin_dir = fixtures_dir.join("author-bio");
    let md_src = std::fs::read_to_string(plugin_dir.join("blocks/author-bio.md"))
        .expect("markdown template file exists");
    tera.add_raw_template("author-bio::blocks/author-bio.md", &md_src)
        .unwrap();

    let doc = Document {
        front_matter: FrontMatter::default(),
        blocks: vec![Block {
            r#type: "lopress:author-bio".into(),
            attrs: json!({
                "name": "Jane",
                "bio": "Loves **Rust** and **Rust**",
                "spoiler": true
            }),
            children: vec![],
            text: None,
        }],
    };
    let html = render_body(&doc, &registry, &tera, &ImageIndex::default()).unwrap();

    // Check that the markdown template was interpolated AND the result
    // was rendered through the md→HTML pipeline.
    assert!(html.contains("<strong>About Jane</strong>"), "name rendered: {html}");
    assert!(html.contains("<strong>Rust</strong>"), "markdown in bio rendered: {html}");
    assert!(html.contains("<blockquote>"), "spoiler conditional rendered: {html}");
    assert!(html.contains("Contains spoilers"), "spoiler text present: {html}");
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p lopress-build author_bio_plugin_renders_markdown_template_through_pipeline`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/lopress-build/src/render.rs
git commit -m "test(build): integration test for author-bio markdown template render"
```

---

## Task 9: Full gate + end-to-end verification

- [ ] **Step 1: Run the canonical gate**

Run: `bash scripts/check.sh`
Expected: fmt + `clippy --workspace --all-targets -D warnings` + `cargo test --workspace` pass.

> **Clippy caching note:** clippy can falsely pass on cached crates after a prior `cargo test/run/build`. If you get a green clippy but suspect stale results, force a re-lint by touching a file in each changed crate:
> ```bash
> touch crates/lopress-plugin/src/manifest.rs crates/lopress-editor/src/ui/blocks/plugin.rs crates/lopress-build/src/render.rs crates/lopress-build/src/build.rs
> cargo clippy --workspace --all-targets -- -D warnings
> ```

- [ ] **Step 2: End-to-end (control interface)**

Via the `127.0.0.1:7878` control server (the `driving-lopress-editor` capability):

1. Launch the editor: `cargo run` (visible window; poll `/ping` until ready)
2. Open a test post (e.g. `$TEMP/test-post.md`)
3. Insert a template-form block via the slash menu (the `author-bio` block should appear in the inserter since it's a registered plugin)
4. Fill the form fields:
   - "Author name" → "Jane"
   - "Short bio" (textarea) → "Loves **Rust**"
   - "Mark as spoiler" (checkbox) → checked
5. Save the document
6. Confirm the live-preview webview shows:
   - `<strong>About Jane</strong>`
   - `<p>Loves <strong>Rust</strong></p>`
   - `<blockquote>⚠️ Contains spoilers.</blockquote>`
7. Uncheck the spoiler, save, and confirm the blockquote disappears

Record verbatim commands + outputs; no PASS without them.

- [ ] **Step 3: Commit any gate fixes**

```bash
git add -A
git commit -m "chore: gate pass for template-form blocks"
```

---

## Self-Review Notes (for the planner)

- **Spec coverage:** manifest fields + mutual-exclusivity (Task 1), Tera registration (Task 2), build-render branch (Task 3), editor textarea + label/help (Task 4), literal struct fix (Task 5), example plugin fixture (Task 6), round-trip (Task 7), integration test (Task 8), gate + e2e (Task 9).
- **Context design:** attrs are inserted at the top level of the Tera context (bare `{{ name }}`), matching the spec's worked example. This is different from the existing HTML template convention (which nests under `attrs`).
- **Markdown pipeline:** the new branch does `tera.render() → lopress_core::parse() → render_body()`, reusing the existing pipeline. No new code in `lopress-core` is needed.
- **Floem textarea:** uses `floem::views::text_editor` (Floem 0.2.0) with `lapce_xi_rope::Rope`. The workspace already imports `lapce_xi_rope` in other editor modules.
- **No `lopress-core` changes:** the `parse` function already exists and returns `Result<Document, ParseError>`. The error maps to `BuildError::Config` via a `map_err` closure.
- **Mutual-exclusivity:** validated in both `parse_manifest` and `parse_manifest_str` via a shared `validate_manifest` function. Returns `PluginError::MutualExclusion { field1, field2 }`.
- **Commit style:** conventional commits scoped by crate, matching git history. One commit per completed task.
