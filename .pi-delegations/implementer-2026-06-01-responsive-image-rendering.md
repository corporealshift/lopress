# Implementer delegation — Responsive Image Rendering

**You are pi, a competent mid-level Rust engineer with git + the full toolchain.**
Implement the plan at `docs/superpowers/plans/2026-06-01-responsive-image-rendering.md`
**in full**, task by task (Tasks 1–7). Use your local `writing-plans` / TDD discipline:
write the failing test first, make it pass, commit per task exactly as the plan specifies.

## How to work

- **REQUIRED SKILL:** treat this as `executing-plans` — follow the plan's checkboxes in
  order, run each step's stated command, and don't skip the per-task commits.
- **Real code wins.** The plan was written today against the current `HEAD`
  (`feat/responsive-images`), so cited snippets are close — but still grep/read the
  real construct before editing. If a snippet can't be mapped confidently, STOP and
  report rather than hand-balancing braces or force-matching text.

## Confirmed facts (already verified — don't re-derive)

- `lopress_assets` re-exports everything you need from the crate root
  (`crates/lopress-assets/src/lib.rs:25`):
  `pub use image::{process_image, ImageResult, Variant, VariantSpec};` and
  `pub use cache::{hash_file, VariantCache};`. The plan's `use lopress_assets::ImageResult;`
  / `use lopress_assets::Variant;` imports are valid as written.
- `Variant` fields (`crates/lopress-assets/src/image.rs:30`): `filename: PathBuf`,
  `width: u32`, `format: String` ("webp" or original ext). `ImageResult { files: Vec<Variant> }`.
  No `as` casts needed anywhere — `width` is already `u32` and is used in string formatting.
- Current `image` render arm is `crates/lopress-build/src/render.rs:93-97` and emits a bare
  `<img src="{src}" alt="{alt}">` (no `<figure>`, no `loading`). Replace it with the
  `write_image` helper per Task 2.
- `write_block` recurses in three places that must each gain the `image_index` argument:
  quote children (`render.rs:61`), list-item children (`render.rs:87`), and
  `render_custom` inner children (`render.rs:128`).
- Image pipeline currently lives at `crates/lopress-build/src/build.rs:190-211` and sits
  **after** the `pages::render_all(...)` call (line ~97). Task 4 moves it **before**
  `render_all`. Note `failures` is the `Vec<PageFailure>` the image block already pushes to —
  before you move the block, confirm `failures` is declared *above* the new (earlier) location;
  if not, move its declaration up. Keep the block after the `force_full` www wipe (`build.rs:49`).

## ⚠️ CRITICAL — cross-plan reconciliation (the read-more plan already landed)

The plan's "Cross-plan note" is now **live**: `render_excerpt` **exists** in
`crates/lopress-build/src/render.rs:24-40` and it calls `write_block`. You MUST:

1. Add `image_index: &ImageIndex` to `render_excerpt` (same as `render_body`) and pass it
   into its `write_block` call.
2. Update `render_excerpt`'s caller at `crates/lopress-build/src/pages.rs:81`
   (`crate::render::render_excerpt(&p.doc, registry, tera)`) to pass the threaded
   `image_index`. Whatever function contains that call must therefore also receive
   `image_index` — thread it from `render_all` down (it's the same call chain as
   `render_one_post`/`render_one_page`).
3. Update the in-crate `render_excerpt` tests (`render.rs:214`, `:224`) and `render_body`
   tests (`:195`, `:235`, `:251`, `:296`) to pass `&ImageIndex::default()`.

Do a full sweep: after changing the signatures, `cargo test -p lopress-build --no-run` will
list every stale call site — fix them all (empty `&ImageIndex::default()` for tests that
don't exercise images) before moving on.

## Lints (AGENTS.md — clippy `--workspace --all-targets -D warnings`)

- No `unwrap`/`expect`/`panic`/`unreachable`/`unimplemented`/`todo` in production code
  (tests are exempt via the crate's `#![cfg_attr(test, allow(...))]`).
- No lossy `as` casts. (None needed here — see facts above.)
- `render_all` already has `#[allow(clippy::too_many_arguments)]`; adding one more arg is fine.
  If you find another fn crossing the arg threshold, prefer bundling into a struct over a
  wide arg list, but don't over-engineer — a single `&ImageIndex` added to existing fns is fine.
- Justify any new `#[allow]`/`#[expect]` with a comment.

## The gate (run once, before declaring done)

```bash
bash scripts/check.sh
```

This is the canonical gate: `cargo fmt` + `cargo clippy --workspace --all-targets -D warnings`
+ `cargo test --workspace`. **Note:** per the repo's known clippy-cache false-pass, if you ran
`cargo test`/`build`/`run` just before, the clippy step may skip up-to-date crates and show a
false green — force a real re-lint (`touch crates/lopress-build/src/lib.rs` or `cargo clean -p
lopress-build lopress-theme`) before trusting the clippy result. Stage any fmt changes.

## When done

Report back per task: the test command(s) you ran and their verbatim PASS output, the commits
you made, and the result of `bash scripts/check.sh`. If you hit anything the plan didn't predict
(a snippet that wouldn't map, an unexpected caller, a clippy lint the plan didn't mention),
surface it explicitly rather than papering over it.
