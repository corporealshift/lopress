# Agent guidance for this repo

Most of these rules are enforced by `[workspace.lints]` in `Cargo.toml` (see
that file for the canonical list). This document explains intent and covers
rules clippy can't check.

## Lint suppressions must be justified

Never add `#[allow(...)]`, `#[expect(...)]`, `#[allow(clippy::...)]`, or any
similar opt-out (including `// rustfmt::skip`, `cargo clippy -- -A ...`, and
`#[cfg(test)] #[allow(dead_code)]`) without a short comment on the same item
explaining **why** the lint is wrong here. If you can't articulate a reason,
the suppression isn't warranted — fix the code instead.

Format:

```rust
// Public for integration tests; internally only constructed via `from_foo`.
#[allow(dead_code)]
pub fn bar() { ... }
```

Review tip: grep for `#\[allow` / `#\[expect` before merging; every hit must
have an adjacent justification. No clippy lint inspects adjacent comments, so
this is policy, not tooling.

## Prefer pattern matching over if/else on discriminants

When branching on `Option`, `Result`, enum variants, or any value that can be
destructured, reach for `match` or `if let`/`let ... else` instead of
`if x.is_some() { ... }` / `if x.is_ok() { ... }` / tag-check ladders. Pattern
matching surfaces exhaustiveness, binds the inner value once, and keeps the
happy path unindented.

Prefer:

```rust
let Some(value) = maybe else { return Ok(()) };
match bucket {
    Bucket::Source => push(&mut cs.sources, path),
    Bucket::Plugins => push(&mut cs.plugins, path),
    Bucket::Config => cs.config = true,
    Bucket::Ignored => {}
}
```

Avoid:

```rust
if maybe.is_some() {
    let value = maybe.unwrap();
    ...
}
if bucket == Bucket::Source {
    ...
} else if bucket == Bucket::Plugins {
    ...
}
```

Plain `if cond { ... } else { ... }` on a boolean predicate is fine — this rule
is about destructuring, not about banning `else`. Clippy catches some cases
(`single_match`, `redundant_pattern_matching`, `unnecessary_unwrap`) but not
all of them; judgment still applies.

## No `.unwrap()` or `.expect()` in production code

Both are denied by clippy (`unwrap_used`, `expect_used`). Propagate with `?`,
destructure with `let ... else`, or return a real error. Tests exempt these
via `#![cfg_attr(test, allow(...))]` at each crate root — that's deliberate;
a panicking test is a failing test.

The same reasoning extends to `panic!`, `todo!`, `unimplemented!`,
`unreachable!`, direct indexing (`arr[i]` — use `.get()`), UTF-8-unsafe byte
slicing (`s[..n]`), integer division (`/` on ints — usually a sign you wanted
`div_euclid` or a float), and `std::process::exit` (let `main` return an
error instead). All denied.

## No lossy `as` casts on numeric types

`as` silently truncates, sign-flips, and loses precision. Use `From` / `Into`
for infallible widening (`u32::from(byte)`) and `TryFrom` / `try_into` for
narrowing that needs a check. Clippy denies `cast_possible_truncation`,
`cast_sign_loss`, `cast_possible_wrap`, `cast_precision_loss`, and
`cast_lossless`.

If a cast truly is safe in context (e.g., you just checked the bound), add an
`#[allow]` with a one-line justification per the suppressions rule.

## Don't `.clone()` to appease the borrow checker

A reflexive `.clone()` to silence an error is a smell — you usually want to
pass a borrow, take `&self` instead of `self`, or rework the lifetime. Reach
for clone when the semantics genuinely call for an owned copy (storing into a
struct, spawning a thread) or when the type is a cheap clone (`Arc`, `Rc`,
`Copy`). No lint enforces this; use judgment and favor ownership redesign.

## Newtype domain-meaningful values

Public signatures should use newtypes for semantically-distinct values — a
slug is not an arbitrary `String`, a port is not an arbitrary `u16`, a base
URL is not an arbitrary `String`. Wrapping the primitive in a tuple struct
(`struct Slug(String)`, `struct BaseUrl(Url)`) prevents accidental mixing at
call sites and gives the type one place to live for validation. Cost is a
little boilerplate; benefit is that `fn link(slug: &Slug, page: &Slug)` can't
be called with the arguments reversed.

No lint for this — it's an architectural choice on new code.

## Document panics, use `debug_assert!` for invariants

Any function that can panic in release mode must describe when in a `#
Panics` doc section. Clippy warns on missing sections (`missing_panics_doc`,
`missing_errors_doc`).

For invariants that are impossible by construction — "this index is valid
because we just pushed" — use `debug_assert!` rather than a release-time
check. Release builds stay lean; tests and dev builds still catch
regressions.

## No `unsafe`

`unsafe_code` is forbidden at the workspace level. If a task genuinely needs
`unsafe`, raise it before writing the code — the answer is usually to reach
for a crate that has already vetted the unsafe block.
