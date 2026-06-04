# CI Hardening — Release Tests, `-Dwarnings`, Suppression Gate, TOML Format

**Date:** 2026-06-04
**Author:** Kyle
**Status:** draft — design review output, pending implementation planning
**Related:**
- `.github/workflows/ci.yml` (the workflow extended here)
- `scripts/check.sh` + `AGENTS.md:127-156` (the single-source-of-truth gate principle)

---

## 1. Background

CI today (`.github/workflows/ci.yml`) runs `fmt --check`, `clippy --workspace -D
warnings`, and `test --workspace` across a 3-OS matrix, then builds release artifacts.
The 2026-06-04 review identified gaps; this spec implements the **four** the user
selected:

1. `cargo test --release` — release-profile behavior is currently never tested.
2. `RUSTFLAGS=-Dwarnings` — `cargo build`/`cargo test` pass with rustc (non-clippy)
   warnings today; only the clippy step fails on warnings.
3. **Automate the `#[allow]` justification policy** — currently "policy, not tooling"
   (`AGENTS.md:23-25`).
4. `taplo fmt --check` — TOML formatting, matching the Rust-side `fmt --check` discipline.

Explicitly **deferred** (reviewed, not chosen): the headless editor smoke test, MSRV
job, `cargo-deny`, `cargo doc -Dwarnings`. Noted so the record is complete; not in scope.

---

## 2. Guiding constraint: keep `check.sh`, the Stop hook, and CI in lock-step

`AGENTS.md` is emphatic that `scripts/check.sh`, the Stop hook, and the documented gate
share one source of truth so they can't drift (`AGENTS.md:127-156`). Three of the four
items belong in `check.sh` so the local gate and the Stop hook enforce them too:
`-Dwarnings`, the suppression gate, and `taplo`.

The **one intentional exception** is `cargo test --release`: it roughly doubles
compile+test time, which is too heavy to run on every agent Stop. It is **CI-only**, with
an explicit comment in both `check.sh` and `ci.yml` explaining the divergence (it adds
*coverage* in CI; it does not let the local debug gate falsely pass). This is the only
sanctioned break from "check.sh == CI" and must be documented as such.

---

## 3. Item 1 — `cargo test --release`

### Why
The codebase enforces invariants with `debug_assert!`
(`crates/lopress-editor/src/actions.rs:978`) and gates the entire control server behind
`#[cfg(debug_assertions)]`. CI only ever runs the debug profile, so any release-only
divergence (a `debug_assert` masking a real bug, a `cfg(debug_assertions)` code path that
changes behavior) ships untested.

### Change
Add a release test step to the existing `test` job, after the debug test:

```yaml
      - run: cargo test --workspace
      - name: Release-profile tests (debug_assert / cfg(debug_assertions) divergence)
        run: cargo test --workspace --release
```

Note: in `--release`, `debug_assert!` is compiled out and `cfg(debug_assertions)` is
false, so this run exercises a genuinely different build. Keep both runs — they cover
different things.

---

## 4. Item 2 — `RUSTFLAGS=-Dwarnings`

### Why
`cargo clippy -- -D warnings` only fails on warnings *during the clippy invocation*.
Plain `cargo build` / `cargo test` happily compile with rustc warnings (unused imports,
dead code, deprecations). Promote them.

### Change
Set it globally in the workflow `env` block so every cargo invocation inherits it:

```yaml
env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-Dwarnings"
```

And export it in `scripts/check.sh` (so the local gate and Stop hook match):

```bash
export RUSTFLAGS="${RUSTFLAGS:--Dwarnings}"
```

### Caveat for the planner
Toggling `RUSTFLAGS` between invocations invalidates the build cache. Setting it once for
the whole script/workflow (as above) is consistent across all steps and avoids
thrashing. Verify the workspace currently builds clean under `-Dwarnings` before gating —
fix any latent rustc warnings as the first task (TDD: turn the flag on, see what breaks,
fix, then commit the flag).

---

## 5. Item 3 — Suppression justification gate

### The policy (from `AGENTS.md:7-25`)
Every `#[allow(...)]`, `#[expect(...)]`, `// rustfmt::skip`, and clippy `-A` must have a
short comment explaining **why** the suppression is correct, on or adjacent to the item.

### The check
New script `scripts/check-suppressions.sh` (bash; CI runs on Linux, matching `check.sh`):

Contract:
- Scan every `.rs` file under `crates/*/src/`, `crates/*/tests/`, and `src/`.
- For each line matching `#\[allow\(` or `#\[expect\(` or `#!\[allow\(` /
  `#!\[cfg_attr\(test, allow`: require a `//` comment **on the same line** or on a
  **contiguous comment block immediately above** the attribute (the lines directly
  preceding, until a non-comment line).
- Also flag any `// rustfmt::skip` without an adjacent justification.
- Exit non-zero listing each unjustified suppression as `path:line: <text>`.

### The test-suppression question (resolve before gating)
The repo blesses test panic-exemptions via `#![cfg_attr(test, allow(unwrap_used, …))]` at
crate roots (`AGENTS.md:69-71`). Many per-test-module `#[allow(clippy::unwrap_used,
clippy::expect_used, clippy::unreachable, clippy::indexing_slicing, clippy::panic)]`
exist (e.g. `actions.rs:404,430,448`). **Proposed rule:** the gate exempts *exactly* this
canonical test-panic set when the `allow` sits inside a `#[cfg(test)]` module, since the
crate-root `cfg_attr` already documents the rationale once for the whole crate. Every
*other* suppression — including any non-panic allow in test code and all production
allows — needs a per-item justification.

This keeps the gate from drowning in the blessed test exemptions while still enforcing the
policy where it matters. The planner confirms the exact exempt set against the crate-root
`cfg_attr`s during implementation.

### Rollout (TDD order)
1. Write `check-suppressions.sh`.
2. Run it against the repo. It will list current unjustified suppressions.
3. For each: add the missing one-line justification, or delete the suppression if it
   isn't warranted (the policy's own instruction). This is the bulk of the work — a
   cleanup pass, not just a script.
4. Once the repo is clean, wire the script into `check.sh` and `ci.yml`.
5. Commit the gate only after the repo passes it.

### Change to `check.sh`
```bash
echo '=== suppression justifications ==='
bash scripts/check-suppressions.sh || failed=1
```
And a matching CI step. The script is the single source; both call it.

---

## 6. Item 4 — `taplo fmt --check`

### Why
Rust formatting is gated (`cargo fmt --check`); TOML (`Cargo.toml`, the workspace
members, every `plugin.toml` / `manifest.toml`) is not. `taplo` is the standard TOML
formatter.

### Change
CI step (install + check):

```yaml
      - name: Install taplo
        run: cargo install taplo-cli --locked
      - run: taplo fmt --check
```

`cargo install taplo-cli` is slow; rely on `Swatinem/rust-cache` (already present) to
cache it, or pin a prebuilt binary action if install time is a problem. Add a
`taplo.toml` at the repo root only if the default style needs adjusting — start with
defaults and a `taplo fmt` pass to establish the baseline.

Local `check.sh`:
```bash
echo '=== taplo fmt ==='
taplo fmt --check || failed=1
```
Gate this behind a `command -v taplo` guard so a contributor without taplo gets a clear
"install taplo" message rather than a confusing failure — but CI always has it.

### Rollout
Run `taplo fmt` once to normalize all TOML, commit that, *then* add the `--check` gate
(same pattern as the suppression gate: make the repo pass before fencing it).

---

## 7. Final `ci.yml` shape (test job)

```yaml
env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-Dwarnings"

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: rustfmt, clippy }
      - uses: Swatinem/rust-cache@v2
      - name: Install Linux system deps
        if: runner.os == 'Linux'
        run: |
          sudo apt-get update -y
          sudo apt-get install -y \
            libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
            libxkbcommon-dev libgtk-3-dev
      - run: cargo fmt --check
      - name: Install taplo
        run: cargo install taplo-cli --locked
      - run: taplo fmt --check
      - name: Suppression justification gate
        if: runner.os == 'Linux'   # bash script; one OS is enough
        run: bash scripts/check-suppressions.sh
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - run: cargo test --workspace --release   # CI-only; see scripts/check.sh note
```

(`timeout-minutes` and a `concurrency: cancel-in-progress` block are cheap hygiene wins
the planner should fold in while editing the file.)

---

## 8. Testing

CI changes are verified by CI itself, but the rollout has testable units:

- **`check-suppressions.sh` behavior**: a fixture with (a) a justified allow, (b) an
  unjustified allow, (c) a blessed test-panic allow → the script passes (a) and (c),
  fails (b). Worth a tiny shell test or a checked-in fixture exercised in the script's
  own self-test, since the gate's correctness gates the whole repo.
- **Repo passes each new gate** before it's wired in (the rollout steps in §5/§6).
- **`-Dwarnings` clean build**: the workspace compiles warning-free under the flag (the
  first task of Item 2).

---

## 9. Non-goals

- Headless editor smoke test (`xvfb-run`) — explicitly deferred by the user.
- MSRV job, `cargo-deny`, `cargo doc -Dwarnings` — reviewed, not selected.
- No change to the existing `build`/artifact job beyond inheriting the `env` block.

---

## 10. Decisions

### Three of four gates go in `check.sh`; release-test is CI-only
The repo's single-source-of-truth rule means `-Dwarnings`, suppressions, and taplo must
live in `check.sh` so the Stop hook enforces them. `cargo test --release` is the sole
documented exception — too slow per-Stop, and it only *adds* coverage, never masks a
debug failure. Both files carry a comment explaining the one divergence.

### Suppression gate exempts the canonical test-panic set, enforces everything else
The crate-root `cfg_attr(test, allow(...))` already blesses test panics once; re-justifying
them per-module is noise. Every other suppression carries its reason, per policy.

### Make the repo pass each gate before wiring it in
Both the suppression gate and taplo land in two moves: normalize/justify the repo, commit;
then add the check. Avoids a red CI on the same PR that introduces the gate.

### Single shared script for the suppression gate
`scripts/check-suppressions.sh` is called by both `check.sh` and `ci.yml` — same pattern
as `check.sh` being the one gate. No reimplementation in YAML.

---

## 11. Open questions for the planner

- **taplo default style**: accept taplo's defaults, or add a `taplo.toml` to match the
  current hand-formatting of `Cargo.toml`? Proposal: run `taplo fmt`, eyeball the diff;
  if it's churny, add a minimal `taplo.toml` to preserve the existing style.
- **Suppression gate on all three OSes or Linux-only**: it's a bash script reading source;
  one OS suffices and avoids Windows bash friction. Proposal: `if: runner.os == 'Linux'`.
- **taplo install cost**: if `cargo install taplo-cli` dominates CI time even with cache,
  switch to a prebuilt-binary action. Measure first.
