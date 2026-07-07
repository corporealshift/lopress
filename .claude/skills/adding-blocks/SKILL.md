---
name: adding-blocks
description: Use when asked to add, create, or implement a new block type for lopress (callout, embed, gallery, chart, etc.) — or when deciding whether a block should be a site plugin or a built-in base plugin.
---

# Adding a Block Type to lopress

## Overview

lopress has two block mechanisms with very different costs. Picking wrong is expensive in one direction (writing cross-crate Rust for something a 20-line TOML + template handles) and impossible in the other (a site plugin cannot get a custom editor widget or native markdown output).

**Default to a site plugin. Only build a built-in when the block needs something a site plugin cannot do. If the decision is unclear, ask the user before writing any code.**

## The decision

Make the block a **built-in (base plugin)** only if at least one of these holds:

- It needs a **dedicated interactive editor widget** — direct manipulation that a generic attr form (text/textarea/select/checkbox/number fields) cannot express. Tables, lists, and the code editor are built-ins for this reason.
- It must serialize as **native markdown** (` ``` ` fences, `|` tables, `---`, `![](…)`) instead of a `<!-- lopress:… -->` comment container, so the `.md` stays portable.
- It needs **core parser/renderer involvement** (new markdown construct, image pipeline, excerpt marker).

Everything else — attrs interpolated into an HTML or markdown template — is a **site plugin**. Note the audience difference too: a site plugin ships in one site's `plugins/` directory; a built-in ships in the binary for every lopress user.

**Gray area?** (e.g. "gallery" — attr form might do, or might want drag-drop image management): present both options with the trade-off and ask the user. Don't silently pick built-in.

## Path A: site plugin (no Rust)

Full reference: `docs/plugins.md` (fields, flavors, worked examples — callout, button, embed, pullquote, spacer, audio, video, file).

1. Create `<site>/plugins/<name>/plugin.toml` with a `[[blocks]]` entry named `lopress:<name>`, plus the template file. Choose the flavor: `template` (Tera → HTML, attrs under `{{ attrs.* }}`, body via `{{ inner_html | safe }}`) or `markdown_template` (Tera → markdown → HTML pipeline, attrs at top level `{{ name }}`). They are mutually exclusive.
2. Declare every attr with `type`, `ui`, `label`, and a `default` where sensible.
3. **Traps:**
   - The attr form pairs declarations to stored values **by position** — a block's stored attrs must include *every* declared attr or labels and values misalign. The inserter seeds them automatically; hand-authored markdown must carry all fields.
   - Fresh plugin blocks are Inline-bodied (`/state` shows them as `kind:"paragraph"`), not Opaque.
   - `.html` templates auto-escape interpolations; use `| safe` only for trusted HTML like `inner_html`.
4. Verify on a **scratch site** (`cargo run --quiet -- new $env:TEMP\lopress-scratch`, then copy the plugin in — never the user's real site): insert via slash menu, check the persisted `.md` round-trips, and check the rendered HTML in `www/`. The `driving-lopress-editor` skill covers driving the GUI.

## Path B: built-in base plugin (Rust, cross-crate)

Follow the order below; each step's file is the authoritative example (table and separator are the most recent complete additions). End every task with `bash scripts/check.sh`.

1. **Manifest:** `base_plugins/<name>/manifest.toml` (`builtin = true`, `editor = "<key>"`, `native = "<core type>"` if it serializes as native markdown), and add its `include_str!` to `load_base_plugins` in `crates/lopress-plugin/src/registry.rs`.
2. **Body shape:** reuse an existing `BlockBody` variant if possible (`Inline`, `Code`, `List`, `Table`, `Opaque`). A genuinely new shape means a new variant in `crates/lopress-editor/src/model/types.rs` plus an `EditorBlock::<name>()` constructor — and touches every `match` on `BlockBody` across the workspace.
3. **Descriptor:** add an `EDITOR_<NAME>` const and a `BlockDescriptor` entry in `crates/lopress-editor/src/model/descriptor.rs` (editor key, native claim, body shape, `builtin`, slash/toolbar `MenuEntry`s, `default_block`). Menus and ChangeType are projected from this table — no separate menu wiring.
4. **Widget:** `crates/lopress-editor/src/ui/blocks/<name>.rs`, registered in `editor_for` in `ui/blocks/editor_registry.rs`. A test enforces descriptor↔`editor_for` parity, so forgetting this fails the gate.
5. **Round-trip:** add arms in `native_block_from_core` (`model/from_core.rs`) and `native_block_to_core` (`model/to_core.rs`). **Dispatch on the descriptor editor key, never on body shape** — distinct types share shapes (image and separator). If the block introduces a new markdown construct, extend `parser.rs`/`serializer.rs` in `lopress-core` first.
6. **Build output:** add a `write_block` arm in `crates/lopress-build/src/render.rs` so the published site renders it (the editor working proves nothing about `www/`).
7. **Control server:** if you added a body shape, extend `serialize_state` in `crates/lopress-editor/src/ctrl/mod.rs`; optionally add `CtrlAction` support so the block is drivable in e2e verification.
8. **Tests:** round-trip tests must assert `plugin.editor` identity, not just output equality — `Opaque` stashes unknown JSON verbatim, so a missed dispatch arm still round-trips cleanly and hides the bug. Add `apply`/undo tests for any new actions.
9. **Verify live** on a scratch site via the `driving-lopress-editor` skill: insert from the slash menu, edit, save, and read both the persisted `.md` and the rendered `www/` HTML.

## Red flags

- Writing Rust before the site-plugin option was ruled out (or the user chose).
- A "built-in" whose editor is just a form of text fields — that's a site plugin.
- Round-trip test passes but never asserts `plugin.editor` — it may be testing the Opaque fallback.
- New block renders in the editor but `render.rs` was never touched — the published site shows an `<!-- unknown block -->` comment.
