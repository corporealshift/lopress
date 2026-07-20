# Sync post filenames to the slug

**Status:** design approved, awaiting spec review
**Date:** 2026-07-17

## Problem

New documents are created as `untitled-1.md`, `untitled-2.md`, … and the
filename never changes afterward. Even after the author gives a post a real
title and slug, the `untitled-N` stem lingers on disk. Because a post's URL
slug defaults to the filename stem (`front_matter.slug` if set, else the
stem), these placeholder names also leak into the built site's URLs when no
explicit slug is set.

We want the `.md` filename to stay synchronized with the post's slug so the
filesystem reads the way the site does.

## Effective slug

Define the **effective slug** of a document as:

- `slugify(front_matter.slug)` when the Slug field is non-empty, else
- `slugify(front_matter.title)`.

The `.md` filename stem is kept equal to the effective slug. Front matter is
**never** mutated by this feature — only the filename moves. (The build
already resolves the URL slug from `front_matter.slug` else the stem, so once
the stem tracks the effective slug the two agree for the common
no-explicit-slug case.)

If the effective slug is empty (both title and slug blank), no rename
happens — the file keeps its current name.

## `slugify` (new, `lopress-core`)

A pure free function `slugify(&str) -> String`:

- Unicode/ASCII lowercase.
- Replace every maximal run of characters that are not ASCII alphanumeric
  with a single `-`.
- Strip leading and trailing `-`.
- Result may be empty (e.g. input was all punctuation) — callers guard for
  this.

Return type is `String` (not a `Slug` newtype); a newtype remains a possible
future follow-up. Chosen for the smallest diff.

Examples:

| input | output |
| --- | --- |
| `"My First Post"` | `my-first-post` |
| `"Hello, World!"` | `hello-world` |
| `"  spaced  out  "` | `spaced-out` |
| `"already-slug"` | `already-slug` |
| `"!!!"` | `` (empty) |

Note: the explicit Slug field is passed through `slugify` too, so a filename
is always filesystem-safe even if the author typed spaces. This can make the
filename differ from a deliberately non-safe `front_matter.slug` (which the
build uses verbatim), but that is already a broken-URL case and out of scope.

## When the sync fires

After **every successful save** — both:

- the debounced autosave (500 ms after the last edit), and
- the synchronous flush that runs on doc-switch (`flush_pending_edits`).

If the computed target stem already equals the current stem, the sync is a
no-op, so steady-state editing renames nothing.

**Known behavior:** pausing >500 ms partway through typing a title renames the
file to the partial slug, then again when typing finishes
(`my-fir.md` → `my-first-post.md`). Intermediate files are transient and the
end state is always correct. Accepted trade for "the filesystem always
matches the slug."

## Collisions

The rename target is resolved to a unique name within the document's own
directory:

- First candidate: `{effective-slug}.md`.
- On collision (a *different* file already exists at that path): append
  `-2`, `-3`, … until free.
- The current file's own path counts as **available**, so a file already
  named `hello-2.md` does not fight itself on the next save, and a target
  that resolves back to the current path is a no-op.

## New-document naming

Replace `unique_untitled_path(dir)` with a slug-based unique-name helper
seeded from the default title:

- New post → `new-post.md` (`slugify("New Post")`), then `-2`, `-3` on
  collision.
- New page → `new-page.md`.

This removes `untitled-N` at the source. The save-time sync then tracks
whatever real title/slug the author enters.

## Mechanics

### Pure core of the rename

`resolve_target(dir, front_matter, current_path, exists_fn) -> Option<PathBuf>`
(location: editor crate, near the save pipeline):

1. Compute the effective slug; if empty → `None`.
2. Resolve a unique target stem within `dir` using `exists_fn` for the
   collision check, treating `current_path` as available.
3. If the resolved target equals `current_path` → `None` (no rename).
4. Else → `Some(target)`.

`exists_fn` is injected so this is unit-testable without touching the disk.

### `EditingState::sync_filename`

New method `sync_filename(&mut self) -> Result<Option<PathBuf>, String>`:

1. Read `current_ref.path` and the live front matter (passed in / read from
   the current doc).
2. Call `resolve_target` with `Path::exists` as `exists_fn`.
3. On `Some(new_path)`: `fs::rename(old, new)` (atomic within the dir),
   update `self.current_ref.path` to `new_path`, return `Ok(Some(new_path))`.
4. On `None`: `Ok(None)`.

`save_doc` is unchanged (still writes content to the current path); the sync
runs *after* a successful save so content is already on disk under the old
name before the rename moves it.

### Wiring at the two save sites

Both the debounced save closure (`start_save_pipeline`) and
`flush_pending_edits` gain access to the `current_path` signal and the
workspace summary signal. After a successful save they call `sync_filename`;
on `Ok(Some(new_path))` they:

- set the `current_path` signal to `new_path` (inspector placeholder +
  sidebar highlight follow), and
- re-scan the workspace and update the workspace summary signal so the
  sidebar row shows the new name.

The rebuild that already runs after each save prunes the stale
`www/posts/{old-slug}/` output (existing stale-HTML pruning path).

## Isolation / boundaries

- `slugify` — pure string → string, no deps. Lives in `lopress-core`.
- `resolve_target` — pure given `exists_fn`; owns collision + no-op logic.
- `EditingState::sync_filename` — the only place that touches the filesystem
  and mutates `current_ref`.
- Signal updates (`current_path`, workspace summary) stay in the two UI save
  sites, matching the existing pattern where `EditingState` owns the truth
  and the UI mirrors it.

## Testing

- `lopress-core`: unit tests for `slugify` (the table above + empty/edge
  cases; idempotence on an already-slugified input).
- Editor crate: unit tests for `resolve_target` — basic derive, explicit-slug
  precedence over title, collision suffixing, current-path-is-available
  no-op, empty-slug `None`.
- `lopress-gui-host` integration test: create a doc, set a title, save, assert
  the on-disk `.md` was renamed to the slug and `current_ref.path` followed;
  a second doc with the same title lands on `-2`.

## Out of scope

- A `Slug` newtype (may follow later).
- Slugifying `front_matter.slug` in the *build* URL path (verbatim today).
- Renaming files that were never opened in the editor (batch/bulk migration
  of existing `untitled-N` files) — only the open document is synced.
