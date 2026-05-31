# Fail-safe block rendering: never render `empty()`, degrade + warn + stay recoverable

**Date:** 2026-05-30
**Status:** open
**Severity:** resilience — turns "content silently disappears and the block is
unrecoverable" into "content stays visible, the user is warned, and the block
can be recovered"

## The problem this fixes

When a block reaches the renderer in a shape the renderer doesn't expect, every
render path responds by drawing **nothing**:

- `ui/blocks/mod.rs:148` — `_ => empty().into_any()` ("Body/kind mismatch —
  render nothing").
- `ui/blocks/plugin.rs:374` — `_ => empty().into_any()`.
- `ui/blocks/editor_registry.rs` — `code_editor_widget` / `list_editor_widget`
  early-return `empty()` when the body shape doesn't match the editor key.

The result is a blank, unfocusable, unselectable gap. The block's content is
still in the model, but the user has no way to see it, edit it, change its type,
or delete it — it is **invisible and unrecoverable** from the UI. This is the
symptom that actually bit a user (the 2026-05-30 toolbar-Code regression: a
stray `EditBlockBody{Inline}` landed on a block that had just become `Code`,
producing `{kind: Code, body: Inline}`, which no render arm matched).

The root harm is **not** that an invalid `(kind, body)` pair can exist — it is
that the renderer's response to *any* unexpected input is silent erasure. A
mismatch from a future code path, a plugin editor that fails at runtime, a
malformed attr, or an unknown block type loaded from disk would all fail the
same invisible way.

## Why graceful degradation, not "make it impossible"

An earlier draft of this idea proposed collapsing `BlockKind` + `BlockBody`
into a single enum so `{Code, Inline}` would be unconstructable (compile-time
totality). That was demoted for two reasons:

1. **The codebase is intentionally open, not closed.** The block-types-as-
   plugins direction (`docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md`)
   is migrating dispatch *off* the `BlockKind` enum onto a manifest/registry so
   block types are runtime-extensible. A closed sum type with total `match`
   re-hardcodes exactly the types that direction is moving into the registry,
   and "the compiler forces every renderer to handle every kind" is impossible
   once kinds are an open set. Runtime render failures are therefore
   **inevitable**, and the right design for inevitable failure is graceful
   degradation + observability, not a guarantee the type system can't make.
2. **It fixes only the failures we can name.** Making one invalid pair
   unrepresentable does nothing for the next unanticipated failure mode. A
   fallback renderer is robust to failures we haven't hit yet.

The enum approach is recorded under "Alternatives considered" below, not chosen.

## The idea: a fallback block view

Replace every `empty()`-on-mismatch site with a single `fallback_block_view`
that **degrades visibly and keeps the block recoverable**. Requirements:

1. **Content stays visible.** Render the block's best-effort text (the existing
   `body_to_flat_text` helper in `actions.rs`, or equivalent) in a readable
   container. The user never loses sight of their content.
2. **The block stays focusable.** Focus is what mounts the toolbar
   (`block_toolbar_for`, keyed on `focus_pub.block`). A focusable fallback means
   the user can still **Change Type** (to recover into a working editor) or
   **Delete** the block. Recovery is the whole point — the fallback is an
   affordance, not a tombstone.
3. **A clear, inline warning.** A persistent banner/badge *on the block*:
   user-facing copy like "This block couldn't be displayed with its editor —
   showing raw content." Not a modal dialog — renders fire constantly and
   reactively, so a blocking popup would re-fire and trap the user. Inline and
   non-blocking, where the problem is.
4. **Loud for developers, but not for bad plugins.** In debug builds, log at the
   fallback site with the offending `(kind, body, plugin.editor)`. A *hard*
   `debug_assert!` belongs only where the cause is an internal invariant we
   control — never on plugin-originated input, which must always degrade
   gracefully (see decision 4 and the provenance caveat). Release builds always
   degrade, never panic.

## What this does *not* cover — keep a thin model guard

The fallback protects **display and recovery**. It does not stop a bad state
from **persisting to disk**: a `{kind: Code, body: Inline}` block can serialize
as a code block with the `lang` silently dropped and the wrong bytes written
(see `docs/superpowers/ideas/2026-05-24-change-type-body-mismatch.md`). That is
a file-integrity symptom, not a display one, and the fallback view says nothing
about it.

So the design is **two complementary layers with different jobs**:

| Layer | Job | Status |
|---|---|---|
| Fallback render (this doc) | Content never disappears; block always recoverable | proposed |
| Model guard at the `apply` chokepoint | A mismatch can't silently corrupt the saved file | the coercion shipped on `feat/editor-ui-review-fixes` (`actions.rs::coerce_body_to_kind`) already does this; the plan (decision 4 below) keeps coercion, tidies the commit sources, then adds a debug-only assert |

Neither layer makes the other redundant. The fallback is the safety net for the
*screen*; the guard is the safety net for the *file*. (A third layer — total,
non-panicking handling at the `from_core` parse boundary — is the safety net for
*bad/missing plugins loaded from disk*; see below.)

## Note on the deeper root cause

The triggering regression was an **event-ordering** problem: `ChangeType` calls
`current_doc.update()`, which rebuilds the whole editor pane, which unmounts the
old inline editor, which fires `FocusLost`, which emits a stray commit *after*
the type change. Neither the fallback render nor the model guard fixes that
ordering hazard — they make its consequences safe. The full-document
`RwSignal<Option<EditorDoc>>` + full-pane-rebuild-per-edit reactivity model is
the soil this class of bug grows in; revisiting render/commit ordering or
reactive granularity is a separate, larger investigation, noted here only so it
isn't mistaken for solved.

## Surface area

Small and incremental — the opposite of the enum refactor:

- **New:** a `fallback_block_view(block, …)` (likely in `ui/blocks/mod.rs` or a
  new `ui/blocks/fallback.rs`) — best-effort text, focusable, warning chrome.
- **Edit:** the three `empty()`-on-mismatch sites — `ui/blocks/mod.rs:148`,
  `ui/blocks/plugin.rs:374`, and the two early-returns in
  `ui/blocks/editor_registry.rs` (`code_editor_widget`, `list_editor_widget`) —
  to call the fallback instead of `empty()`.
- **Debug:** `debug_assert!`/log at the fallback site.
- **Tests:** a UI/integration test that drives a block into a mismatch (e.g. via
  the control server's `/action` replay of the regression sequence, or a direct
  model fixture) and asserts the fallback renders text + is focusable rather
  than producing an empty view. The control server (`ctrl/`) makes this
  observable end-to-end via `/state` + `/screenshot`.

## Resolved decisions

1. **Fallback editability → read-only + toolbar recovery.** The fallback shows
   selectable/copyable text; recovery is via the toolbar (Change Type re-mounts a
   working editor, or Delete). No in-place editing — the fallback exists because
   the body shape is ambiguous, so "commit as the current shape" would risk a
   fresh mismatch. Editable plain-text is a clean follow-up if ever wanted.
2. **Warning → persistent, non-dismissible.** The block *is* still broken, so
   hiding the warning hides a real problem. It self-clears once the block renders
   normally again. Copy: *"This block couldn't be displayed with its editor —
   showing its raw content. Change its type or delete it to recover."*
3. **Plugin-failure parity → fold every dead-end into the one fallback.** Not
   just kind/body mismatch: a plugin declaring an `editor` key with no registered
   widget (`editor_for(key)` returns `None`), an unknown body shape, or any other
   "renderer couldn't handle this" case routes to the same `fallback_block_view`.
   One path, all dead-ends.
4. **Model guard → keep coercion, tidy the commit sources, then assert.**
   Rejection is ruled out: shape-mismatched `EditBlockBody` commits are *routine
   today* (the toolbar pre-commits `Inline` regardless of the focused block's
   kind, `toolbar.rs:78–89`; a `FocusLost` commit fires the editor's own shape
   *after* a `ChangeType` changed the kind), and those commits carry real,
   possibly-uncommitted content — rejecting them is silent data loss. So
   coercion stays (it preserves the text). The work is to **tidy the two commit
   sources** so a mismatch stops being routine — the toolbar pre-commits the
   block's *actual* body shape, and a `FocusLost` commit is suppressed when the
   block's kind changed under it — after which a shape-mismatched commit is
   genuinely abnormal and a `debug_assert!` on coercion catches real bugs in CI.
   This also fixes the sloppy ordering that caused the original regression.

   **Provenance caveat (resilience to bad plugins).** The `debug_assert!` is
   debug-only and targets *internal* invariants we control (built-in commit
   paths, once tidied). **Plugin-originated input must always degrade
   gracefully and never assert/panic** — neither a plugin editor committing a
   wrong-shaped body nor a malformed plugin block loaded from disk may crash the
   editor. Release builds always coerce + degrade; the assertion exists to catch
   *our* bugs in development, not to punish a misbehaving plugin.

## Parse-boundary robustness (a third layer, for bad plugins specifically)

The fallback render and the model guard are both *in-memory* protections. They
cannot make *on-disk* input safe — `from_core` reads markdown that may reference
an unknown `native` type, a plugin that isn't registered, or malformed attrs. A
bad or missing plugin must load into a **recoverable** block (today's `Opaque`
fallback, surfaced through the same `fallback_block_view`), never a panic and
never a vanished block. This is the layer that most directly serves "resilient
to bad plugins": a document authored against a plugin you don't have still opens,
shows its raw content, warns, and stays editable/deletable. Treat total,
non-panicking handling at the `from_core` boundary as a first-class requirement
alongside the fallback view.

## Alternatives considered

- **Unify `BlockKind` + `BlockBody` into one enum** (make `{Code, Inline}`
  unrepresentable). Strong correctness guarantee, but a ~26-file refactor that
  fights the open block-types-as-plugins direction and only closes named failure
  modes. Deferred; not chosen. (Prior detailed write-up lived at
  `2026-05-30-kind-body-invariant-hardening.md`, now removed in favor of this.)
- **Assertion-only** (`debug_assert!` the invariant at `apply`, no UI change).
  Catches regressions in CI but does nothing for a user who hits a mismatch in a
  release build — content still disappears. Folded into this idea as the
  *developer-facing half*, not a standalone fix.

## Related

- `docs/superpowers/ideas/2026-05-24-change-type-body-mismatch.md` — the
  persistence-corruption symptom the model-guard layer addresses.
- `feat/editor-ui-review-fixes` — the shipped model coercion fix
  (`actions.rs::coerce_body_to_kind` / `body_to_flat_text`) and the
  `stale_inline_commit_after_change_to_code_keeps_body_renderable` test. The
  fallback view is the display-layer complement to that model-layer guard.
- `docs/superpowers/specs/2026-05-17-block-types-as-plugins-design.md` — the
  open-extensibility direction that makes graceful degradation the right shape
  of fix.
