# lopress

Desktop blog-authoring tool: a Gutenberg-style block editor (Floem GUI) + static site generator + live-preview server, in one Rust binary. Markdown on disk is the source of truth.

## Rules and verification

@AGENTS.md

## Orientation

- `docs/architecture.md` — crate map, data flow, editor/action/undo model, build pipeline. Snapshot: confirm details against source before relying on them.
- `docs/plugins.md` — the `plugin.toml` manifest reference, with worked examples.
- `docs/themes.md` — theme authoring (template set, context variables, Tera gotchas).
- Built-in block types are declared once in the descriptor table: `crates/lopress-editor/src/model/descriptor.rs`. Base-plugin manifests live in `base_plugins/`.
- Design specs and implementation plans live under `docs/superpowers/specs/` and `docs/superpowers/plans/` (dated files; newest = most current thinking).

## Project skills (in `.claude/skills/`)

- `adding-blocks` — **read before adding any block type**; decides site plugin vs built-in and carries both checklists.
- `driving-lopress-editor` — the debug HTTP control server for seeing/driving the running GUI.
- `building-on-floem` — how this codebase uses floem + known layout/hit-test/focus traps; read before writing or debugging editor UI.
- `verifying-lopress-work` — evidence rules for reporting anything as working/fixed/verified.

## Working on the editor GUI

- To see, drive, or verify the running editor, use the `driving-lopress-editor` skill (debug HTTP control server on 127.0.0.1:7878: `/state`, `/screenshot`, `/action`, `/input`, `/click`).
- **Never drive edits against a real site or committed fixture** — `/action` and autosave persist to disk. Scaffold a scratch site with `cargo run --quiet -- new $env:TEMP\lopress-scratch` first.
- Run with plain `cargo run` from the repo root (debug build; the control server is compiled out of `--release`, and `cargo run -p lopress-editor` fails — it's a library).
