# App icon

**Status:** design approved
**Date:** 2026-10-20

## Problem

lopress ships with no icon, so the taskbar, Alt-Tab, the title bar, Explorer
and pinned shortcuts all show the generic Windows application icon.

## The mark

Concept F from the icon exploration ("monogram as the heading block"): a
near-black rounded square (`#1F1B16`, corner radius 14 on a 64 grid) with a
cream `lp` (`#F4EDE0`, Roboto Slab Bold) in the heading position and two
vermilion paragraph bars (`#C8402A`) beneath it. It reads as both the name and
a page in the block editor.

At 16 and 24 px the full mark's letters smudge, so those sizes use a **small
variant**: a larger `lp` and a single wide bar. 32 px and up use the full mark.

## Assets (`assets/icon/`)

- `lopress.svg`, `lopress-small.svg`: masters. The `lp` is converted to
  outlines from Roboto Slab Bold (Apache-2.0), so rendering never depends on
  installed fonts.
- `lopress.ico`: 16 and 24 from the small master, 32/48/64/256 from the full.
- `lopress-256.png`: the runtime window icon.
- `scripts/build-icon.py`: regenerates the PNG/ICO from the masters. Outputs
  are committed; a normal `cargo build` needs no image tooling. The icon
  changes rarely, so build-time rasterization isn't worth its dependency weight.

## Wiring

1. **Exe resource.** A `build.rs` on the root `lopress` package embeds
   `lopress.ico` via the `winresource` build-dependency, only when
   `CARGO_CFG_TARGET_OS == "windows"`. Covers Explorer, pinned shortcuts and
   the taskbar before a window exists.
2. **Window icon.** `lopress-editor` embeds `lopress-256.png` with
   `include_bytes!`, decodes it with `image` (already a dependency), and passes
   `floem::window::Icon::from_rgba` to `WindowConfig::window_icon`. Covers the
   live window, and non-Windows platforms where an exe resource means nothing.

## Failure

A decode or `Icon` construction failure logs a warning and launches without an
icon. The icon must never block startup.

## Rejected

- *Window icon only:* Explorer and pinned shortcuts stay generic.
- *Exe icon only:* no effect off Windows; the live window's icon isn't
  guaranteed to follow it.
- *`embed-resource`:* needs a hand-written `.rc`; `winresource` takes the
  `.ico` directly.
- *Full mark at 16/24 px:* the letters blur into a smudge.
- *Rasterizing in `build.rs`:* a heavy dependency on every build for a file
  that almost never changes.

## Verification

- Unit test: the embedded PNG decodes to a 256×256 RGBA `Icon`.
- Extract the icon back out of the built `lopress.exe` and inspect it.
- The user confirms the taskbar visually (`/screenshot` captures only the
  window, not the taskbar).
