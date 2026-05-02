# Plugin Code Hooks (Path 2)

**Status:** future direction, not scheduled
**Related:** `docs/superpowers/specs/2026-05-02-editor-floem-design.md` (the editor migration spec implements Path 1)

## Motivation

The editor's plugin system today (Path 1, shipping in the Floem editor) lets a plugin declare a block type, attrs (with `ui` hints), an HTML template for build-time rendering, and which built-in editor kind to reuse for the editor surface. That covers the WordPress-block common case: richer code blocks, video embeds, callout cards, pull-quotes — anything that's mostly a structured set of attrs over a body the user types into a familiar editor.

It does not cover the WordPress-Gutenberg-style case where a plugin ships its own UI: a custom inspector, a custom edit-time rendering, interactive previews, drag-and-drop layouts, etc. To do that a plugin needs to ship code that the editor calls.

This document captures the thinking on how that could work in lopress, so a future spec doesn't start from a blank page.

## Distribution options considered

### A. Compile-in (Rust crates baked into the binary)

Plugins are Rust crates listed in lopress's `Cargo.toml` and compiled into the binary. The trait the plugin implements is part of the lopress source tree.

- *Pros*: trivial to implement, type-safe, no sandboxing needed, no version-skew problems.
- *Cons*: not WordPress-like at all. Adding a plugin requires recompiling lopress. Plugin authors must publish crates and lopress users must trust them at compile time. No "drop a folder in and restart" workflow.

For a personal-use tool, this is actually viable. Long-term, not the right answer.

### B. Native dynamic libraries

Plugins are `.so` / `.dylib` / `.dll` files the editor `dlopen`s at startup. Plugin code calls back into the editor via a stable C ABI.

- *Pros*: native performance, full Rust if both sides agree on ABI.
- *Cons*: ABI matching is brutal — plugins built against a different Rust compiler version, or a different Floem version, break silently or crash. Distribution is per-platform-per-architecture. No sandboxing — a buggy plugin can crash the editor or corrupt files. This is essentially what mature C/C++ apps do for plugins (Photoshop, OBS) but it's expensive engineering.

Probably not the right answer.

### C. WebAssembly

Plugins are WASM modules. The editor exposes a host API as imports the module can call (`paint_text`, `read_attrs`, `write_attrs`, etc.). The plugin exports functions the editor calls (`render_editor`, `handle_event`).

- *Pros*: real sandboxing (plugins can't read arbitrary files or escape), language-agnostic (write plugins in Rust, AssemblyScript, Go, whatever compiles to WASM), version-stable (the host API is the contract, not Rust ABI), distributable (a `.wasm` file is just bytes).
- *Cons*: significant design work to define the host API surface, performance overhead, debugging is harder, plugins can't directly use Floem's painting primitives — they get a constrained subset.

This is the most-WordPress-like answer for the modern era, and it's where this is most likely to land.

### D. Embedded scripting (Rhai, Lua, Mlua, etc.)

Plugins are scripts in a small embedded language.

- *Pros*: simple to add, scripts are easy to author and ship, sandboxing is OK.
- *Cons*: capability is limited — can't easily do complex UI, can't pull in arbitrary deps, performance is worse than WASM. Users have to learn the scripting language.

Useful for plugin glue, not for plugin UI. Could be a complement to WASM later, not a substitute.

## Probable answer

If/when this is built, the answer is most likely **C (WASM)** with a deliberately small host API surface designed around the editor's actual extensibility needs, not "expose all of Floem."

Sketch of what the host API might look like:

- *Read-only state*: `get_block_attrs() -> AttrMap`, `get_block_body() -> Body`, `get_selection() -> Selection`.
- *Mutations*: `set_block_attrs(AttrMap)`, `set_block_body(Body)`, all coalesced and gated through the editor's undo system.
- *Editor surface*: a constrained tree of declarative widgets (Text, Field, Button, Stack, etc.) that the plugin returns from its `render` function. The host paints them. Plugins do not get raw paint access.
- *Events*: `on_event(Event) -> Reaction`. Reactions are restricted to mutations + side effects via the host.

The plugin manifest's existing `editor` field becomes the load hint:

```toml
[[blocks]]
name = "lopress:richcode"
editor = "wasm:./editor.wasm"     # path relative to plugin dir
```

Backward compat with Path 1: values like `"code"` / `"paragraph"` continue to mean "use the built-in editor kind." Values starting with `"wasm:"` mean "load this WASM and call its `render` export."

## Open questions for future spec

- Host API minimum viable surface — exactly which read/write primitives, which widget types.
- Plugin SDK in Rust — what wrapper crate plugin authors compile against. Likely a small `lopress-plugin-sdk` crate that maps a nicer API onto the raw WASM imports/exports.
- Versioning — how the host signals API version, how plugins declare compatibility.
- Plugin security beyond sandboxing — UI capability gates (does this plugin need network? filesystem access to its own dir?).
- Discovery and distribution — does lopress have a plugin registry, or is install always manual?
- Performance budget — how much WASM call overhead per render is acceptable.
- Debugging story — how a plugin author iterates on an editor plugin without restart-loops.

## When this is worth designing

Path 2 is worth a real design pass when one of these is true:

- A concrete plugin idea exists that Path 1 cannot express. (Example: a "table builder" block where the plugin needs to render an interactive grid editor — Path 1's "reuse a built-in editor + attr form" is not enough.)
- A second user starts using lopress and wants to add functionality without recompiling.
- A natural prototype falls out of the WASM-tooling ecosystem (e.g., `wasmtime` host bindings become trivially nice, or a host-API generator emerges).

Until then, Path 1 covers the immediate cases and the forward-compatible `editor` field leaves room for Path 2 without rework.
