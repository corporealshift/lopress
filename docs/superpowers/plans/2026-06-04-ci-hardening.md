# CI Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add four CI/local gates — release-profile tests, `RUSTFLAGS=-Dwarnings`, an automated `#[allow]`-justification check, and `taplo fmt --check` — keeping `scripts/check.sh`, the Stop hook, and `.github/workflows/ci.yml` in lock-step.

**Architecture:** Three gates (`-Dwarnings`, the suppression script, taplo) live in `scripts/check.sh` so the local gate and the Stop hook enforce them; `cargo test --release` is CI-only (too slow per-Stop) with a documented rationale in both files. A new `scripts/check-suppressions.sh` is the single source the local gate and CI both call. Each gate lands in two moves: make the repo pass first, then wire the check in.

**Tech Stack:** GitHub Actions (`.github/workflows/ci.yml`), bash (`scripts/*.sh`), cargo (clippy/test/fmt), `taplo-cli` for TOML, the Rust workspace's `[workspace.lints]` policy + `AGENTS.md`.

---

## File Structure

```
.github/workflows/ci.yml          — MODIFIED: add env, taplo, suppression, release-test, timeout/concurrency
scripts/check.sh                  — MODIFIED: add RUSTFLAGS export, suppression script call, taplo call, release-test comment
scripts/check-suppressions.sh     — NEW: suppression justification gate script
scripts/fixtures/suppress-test.rs — NEW: self-test fixture for the suppression script
```

**Stop hook** (`.claude/settings.json`) — NOT modified. It runs the same three commands as `check.sh` (`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`). The rationale: the Stop hook is a fast gate for the agent's turn; adding `-Dwarnings`, suppression, and taplo checks would make every stop noticeably slower. Running `bash scripts/check.sh` manually is the fuller gate. This decision is documented as comments in both `check.sh` and `ci.yml`.

## Task Order

1. **Task 1: Enable `-Dwarnings` in CI env and `check.sh`** — one commit (the flag is always on after this)
2. **Task 2: Fix all rustc warnings under `-Dwarnings`** — discovery-driven cleanup
3. **Task 3: Add release-profile test to CI** — one commit (CI-only gate)
4. **Task 4: Write `check-suppressions.sh` with self-test** — one commit
5. **Task 5: Justify or remove suppressions flagged by the gate** — discovery-driven cleanup
6. **Task 6: Wire suppression gate into `check.sh` and `ci.yml`** — land-then-gate (two commits: normalize, then wire)
7. **Task 7: Install taplo and normalize all TOML** — land-then-gate (two commits: normalize, then wire)

---

## Standing rules for every task (this repo)

- **Real code wins.** Cited snippets are from the live tree on branch `feat/table-and-separator-blocks` (parent commit `e0991be`) as of 2026-06-04. If a snippet doesn't match disk, grep/read the real construct, apply the *intent*, and STOP-and-report rather than hand-balance.
- **Lints (AGENTS.md).** No `unwrap`/`expect`/`panic`/`unreachable`/`todo`/indexing/`as`-casts/integer-division in production code. Justify every `#[allow]` with an adjacent comment.
- **Gate once, `--workspace`.** Final gate is `bash scripts/check.sh` (fmt + clippy + test). Clippy caches: after a `cargo test/run`, touch a source file in each changed crate before trusting a green clippy.
- **Stage NAMED files per commit — never `git add -A`.** The tree has unrelated untracked files (`.pi-delegations/*`, `.claude/settings.local.json`, `rust-toolchain.toml`). Do not sweep them in.
- **One commit per task**, exactly as each task's commit step lists. For land-then-gate tasks, that's two commits (normalize, then wire).

---

## Task 1: Enable `-Dwarnings` in CI env and `check.sh`

**Files:**
- Modify: `.github/workflows/ci.yml` (add `RUSTFLAGS` to top-level `env` block)
- Modify: `scripts/check.sh` (add `RUSTFLAGS` export near the top, after `set -u`)

**Rationale:** Setting `RUSTFLAGS=-Dwarnings` globally in CI means every cargo invocation (clippy, test, build) inherits it. Setting it in `check.sh` means the local gate and the Stop hook (which runs the same clippy/test commands) enforce it too. The `-Dwarnings` flag is always-on after this commit — no separate "cleanup" commit needed because the gate is introduced and the repo is fixed in two sequential tasks.

- [ ] **Step 1: Add `RUSTFLAGS` to the CI `env` block**

Read `.github/workflows/ci.yml`. Find the existing `env` block:

```yaml
env:
  CARGO_TERM_COLOR: always
```

Edit it to:

```yaml
env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-Dwarnings"
```

- [ ] **Step 2: Add `RUSTFLAGS` export to `check.sh`**

Read `scripts/check.sh`. Find the `set -u` line and add the export immediately after it:

```bash
set -u
export RUSTFLAGS="${RUSTFLAGS:--Dwarnings}"
```

This preserves any user-set `RUSTFLAGS` while defaulting to `-Dwarnings` when unset.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml scripts/check.sh
git commit -m "ci: enable RUSTFLAGS=-Dwarnings in CI and check.sh"
```

---

## Task 2: Fix all rustc warnings under `-Dwarnings`

**Files:** Discovery-driven — any `.rs` file in the workspace that emits a warning under `RUSTFLAGS=-Dwarnings`.

**This is a discovery-driven task.** You cannot pre-write the fixes — the exact warnings only exist when the command runs on the real tree. The per-item decision rule is:

> **For each warning:** Fix it at its source per `AGENTS.md` policy. If it's an unused import, delete it. If it's a dead code warning, add a doc comment or `#[allow(dead_code)]` with a one-line justification. If it's a deprecation, migrate or add `#[allow(deprecated)]` with justification. If it's a cast warning, use `From`/`TryFrom`. **Do NOT suppress warnings with blanket `#[allow]` — fix the code.**

- [ ] **Step 1: Run the discovery command**

```bash
RUSTFLAGS="-Dwarnings" cargo clippy --workspace --all-targets -- -D warnings 2>&1 | head -200
```

Expected: The command will FAIL (exit non-zero) with a list of rustc warnings. Capture the full output.

- [ ] **Step 2: Fix each warning**

For each warning reported:
1. Read the source file at the reported path:line.
2. Apply the minimal fix:
   - **unused imports** → delete the import
   - **dead code** → add `#[allow(dead_code)]` with a one-line comment explaining why (per AGENTS.md suppression rule), or remove the code if it truly isn't needed
   - **deprecated** → migrate to the new API, or add `#[allow(deprecated)]` with justification
   - **cast warnings** → use `From`/`TryFrom`, or add `#[allow]` with justification
   - **unused variables** → prefix with `_` or remove
3. After fixing, re-run:

```bash
RUSTFLAGS="-Dwarnings" cargo clippy --workspace --all-targets -- -D warnings 2>&1 | head -200
```

4. Repeat until the command exits 0 (no warnings).

- [ ] **Step 3: Verify the full gate still passes**

```bash
bash scripts/check.sh
```

Expected: PASS (all three steps green). If it fails, fix the failure and re-run.

- [ ] **Step 4: Commit**

Stage only the `.rs` files that changed:

```bash
git add -u crates src   # tracked .rs modifications under crates/ and src/ only — covers nested dirs, no untracked sweep, excludes unrelated files like .claude/settings.local.json
git commit -m "fix: resolve rustc warnings under RUSTFLAGS=-Dwarnings"
```

> **STOP-AND-REPORT if the warning count is large enough to balloon this task.** If more than ~10 warnings need non-trivial fixes, split this into sub-tasks by crate and report so the plan can be adjusted.

---

## Task 3: Add release-profile test to CI

**Files:**
- Modify: `.github/workflows/ci.yml` (add release test step in the `test` job; add `timeout-minutes` and `concurrency`)
- Modify: `scripts/check.sh` (add a comment explaining why release test is NOT in the local gate)

**Rationale:** `cargo test --release` doubles compile+test time. It exercises a genuinely different build: `debug_assert!` is compiled out and `cfg(debug_assertions)` is false. The spec explicitly keeps this CI-only with documented rationale in both files. This is the only sanctioned break from "check.sh == CI."

- [ ] **Step 1: Add `timeout-minutes` and `concurrency` to the test job**

Read `.github/workflows/ci.yml`. Find the `test:` job line and add timing/concurrency:

```yaml
  test:
    timeout-minutes: 30
    concurrency:
      group: ${{ github.workflow }}-${{ github.head_ref || github.run_id }}
      cancel-in-progress: true
    strategy:
```

- [ ] **Step 2: Add the release test step**

Find the existing `cargo test --workspace` step in the `test` job and add a step after it:

```yaml
      - run: cargo test --workspace
      - name: Release-profile tests (debug_assert / cfg(debug_assertions) divergence)
        run: cargo test --workspace --release
```

- [ ] **Step 3: Add a comment to `check.sh` explaining the divergence**

Read `scripts/check.sh`. Add a comment near the top (after the header, before the `set -u`):

```bash
# NOTE: `cargo test --release` is NOT run here. It is CI-only because it
# roughly doubles compile+test time. The Stop hook runs the same three
# commands as this script (see .claude/settings.json). Running --release
# on every stop would make the agent noticeably slow. The CI step is the
# sole place that exercises the release profile (debug_assert! compiled
# out, cfg(debug_assertions) false) — it adds coverage; it does not mask
# a debug failure.
```

- [ ] **Step 4: Verify the YAML is well-formed**

```bash
python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('YAML OK')" 2>/dev/null \
  || python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('YAML OK')" 2>/dev/null \
  || echo "YAML check skipped (no python+pyyaml available) — inspect .github/workflows/ci.yml indentation manually"
```

Expected: `YAML OK`

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml scripts/check.sh
git commit -m "ci: add release-profile test step (CI-only)"
```

---

## Task 4: Write `check-suppressions.sh` with self-test

**Files:**
- Create: `scripts/check-suppressions.sh` (the gate script)
- Create: `scripts/fixtures/suppress-test.rs` (self-test fixture)

**Self-test design:** The script will run a built-in self-test that creates a temporary fixture file containing three cases: (a) a justified `#[allow]`, (b) an unjustified `#[allow]`, (c) a blessed test-panic `#[allow]` inside a `#[cfg(test)]` module. The self-test asserts the script reports (b) only. The fixture is kept under `scripts/fixtures/` — outside any `cargo` build path — so it doesn't get compiled.

- [ ] **Step 1: Create the `scripts/fixtures/` directory and fixture file**

```bash
mkdir -p scripts/fixtures
```

Create `scripts/fixtures/suppress-test.rs`. The four cases below pin the gate's
behavior. NOTE: a "should-flag" case must have **no comment line directly above the
attribute** — any adjacent comment counts as the justification (per AGENTS.md), so the
descriptive text for flagged cases is separated by a blank line or omitted.

```rust
// A justified allow: this comment is DIRECTLY above the attribute, so the gate
// treats it as the justification. Expected: NOT flagged.
#[allow(dead_code)]
fn justified_fn() {}

#[allow(dead_code)]
fn unjustified_fn() {} // expected: FLAGGED (no comment directly above)

#[cfg(test)]
mod tests {
    // exempt_fn's attribute has no comment above it, proving the exemption is
    // keyed on the lint set, not on an adjacent comment.
    #[allow(clippy::unwrap_used)]
    fn exempt_fn() {
        // expected: NOT flagged (canonical test-panic lint)
        let _ = Some(1).unwrap();
    }

    #[allow(dead_code)]
    fn flagged_in_test() {
        // expected: FLAGGED — dead_code is not in the exempt set, no comment above
        let _ = 42;
    }
}
```

Expected gate output when scanning this fixture: exactly **two** hits — the
`unjustified_fn` and `flagged_in_test` `#[allow(dead_code)]` lines — and **no**
`unwrap_used` line.

- [ ] **Step 2: Write `scripts/check-suppressions.sh`**

Create `scripts/check-suppressions.sh`. The per-file scanning is an `awk` program
(robust to multi-line attributes); the bash wrapper handles file discovery, the
optional explicit-path mode, and the `--self-test`. There is **no** auto-self-test on the
normal scan path (that would recurse) — the self-test is a separate, explicit subcommand
verified in Step 4.

Justification rule (matches AGENTS.md): a suppression is OK if a `//`/`///` comment sits
**directly above** the attribute, **or** a trailing `// ...` comment is on the attribute
line. Exemption rule (no comment needed): the attribute's lint list is **entirely** the
canonical test-panic set — keyed on the lint idents, not on `#[cfg(test)]` scope tracking
(which is unreliable to parse). A bare production allow of only those denied lints would
also pass; that hole is acceptable and documented.

```bash
#!/usr/bin/env bash
#
# Suppression justification gate (AGENTS.md: "Lint suppressions must be justified").
#
# Every #[allow(...)], #[expect(...)], inner #![allow(...)]/#![expect(...)],
# #![cfg_attr(.., allow(..))], and `// rustfmt::skip` must carry an explanatory
# comment — a //-comment line directly above the attribute, or a trailing // on
# the attribute's own line.
#
# Exemption (no comment required): a suppression whose lint list is ENTIRELY the
# canonical test-panic set. The crate-root #![cfg_attr(test, allow(...))] and the
# per-test-module allows of these lints are blessed once in AGENTS.md:
#     unwrap_used expect_used panic unreachable indexing_slicing string_slice
# After stripping `clippy::`, every ident inside allow(...)/expect(...) must be
# one of these. (A bare production allow of only these denied lints would also be
# exempt; acceptable — such an allow is conspicuous in review.)
#
# Usage:
#   check-suppressions.sh                scan crates/*/{src,tests} and src/
#   check-suppressions.sh <path>...      scan the given files
#   check-suppressions.sh --self-test    run the fixture self-test, then exit
#
# Exit 0 = clean. Exit 1 = prints each unjustified hit as `path:line: text`.
set -u

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

scan_one() {
  awk '
    function is_comment(s) { return (s ~ /^[[:space:]]*\/\//) }
    function trim(s) { gsub(/^[[:space:]]+|[[:space:]]+$/, "", s); return s }
    function all_exempt(text,   n, a, i, t, saw) {
      if (text ~ /allow\(/)       sub(/^.*allow\(/, "", text)
      else if (text ~ /expect\(/) sub(/^.*expect\(/, "", text)
      else return 0
      gsub(/clippy::/, "", text)
      gsub(/[()\[\];!,]/, " ", text)
      n = split(text, a, /[[:space:]]+/)
      saw = 0
      for (i = 1; i <= n; i++) {
        t = a[i]
        if (t == "" || t == "test") continue
        if (index(" " EXEMPT " ", " " t " ") == 0) return 0
        saw = 1
      }
      return saw
    }
    function flush(text, lnum) {
      if (text !~ /(allow|expect)\(/) return        # cfg_attr without allow: not a suppression
      if (all_exempt(text)) return
      if (g_above || g_same) return                 # justified by adjacent comment
      printf "%s:%d: %s\n", FILENAME, lnum, g_first
      rc = 1
    }
    BEGIN {
      EXEMPT = "unwrap_used expect_used panic unreachable indexing_slicing string_slice"
      acc = ""; start = 0; rc = 0; prev_comment = 0
    }
    {
      line = $0
      if (start > 0) {                              # mid multi-line attribute
        acc = acc " " line
        if (line ~ /\)[[:space:]]*\]/) { flush(acc, start); start = 0; acc = "" }
        prev_comment = is_comment(line)
        next
      }
      if (line ~ /\/\/[[:space:]]*rustfmt::skip/) { # bare rustfmt::skip needs a reason
        if (!prev_comment && line ~ /rustfmt::skip[[:space:]]*$/) {
          printf "%s:%d: %s\n", FILENAME, FNR, trim(line); rc = 1
        }
        prev_comment = is_comment(line); next
      }
      if (line ~ /#!?\[(allow|expect|cfg_attr)\(/) {
        g_first = trim(line); g_above = prev_comment; g_same = (line ~ /\].*\/\//)
        start = FNR; acc = line
        if (line ~ /\)[[:space:]]*\]/) { flush(acc, start); start = 0; acc = "" }
        prev_comment = is_comment(line); next
      }
      prev_comment = is_comment(line)
    }
    END { exit rc }
  ' "$1"
}

run_self_test() {
  local fixture="$SCRIPT_DIR/fixtures/suppress-test.rs"
  [ -f "$fixture" ] || { echo "self-test: missing fixture $fixture" >&2; return 1; }
  local out; out="$(scan_one "$fixture")"
  if printf '%s' "$out" | grep -q 'unwrap_used'; then
    echo "self-test FAIL: an exempt unwrap_used allow was flagged" >&2
    printf '%s\n' "$out" >&2; return 1
  fi
  local n; n="$(printf '%s' "$out" | grep -c ':' || true)"
  if [ "$n" -ne 2 ]; then
    echo "self-test FAIL: expected 2 flagged lines, got $n" >&2
    printf '%s\n' "$out" >&2; return 1
  fi
  echo "self-test PASS" >&2; return 0
}

if [ "${1:-}" = "--self-test" ]; then
  run_self_test; exit $?
fi

if [ "$#" -gt 0 ]; then
  paths=("$@")
else
  cd "$SCRIPT_DIR/.." || exit 1
  mapfile -t paths < <(
    find crates -type f \( -path '*/src/*.rs' -o -path '*/tests/*.rs' \)
    find src -type f -name '*.rs'
  )
fi

failed=0
for f in "${paths[@]}"; do
  [ -f "$f" ] || continue
  scan_one "$f" || failed=1
done
exit $failed
```

The contract: exit 0 when clean, exit 1 listing each unjustified hit as
`path:line: <text>`. If running it against the real repo in Task 5 surfaces a pattern the
`awk` mishandles (e.g. an unusual attribute layout), fix the `awk` and re-verify the
self-test still passes — do **not** loosen the gate to make a real suppression pass.

- [ ] **Step 3: Make the script executable**

```bash
chmod +x scripts/check-suppressions.sh
```

- [ ] **Step 4: Run the self-test (this is the script's own test — it must pass before committing)**

```bash
bash scripts/check-suppressions.sh --self-test
```

Expected: `self-test PASS` on stderr and exit 0. If it reports FAIL, the `awk` logic is
wrong — fix it against the fixture cases (exactly two `dead_code` lines flagged, the
`unwrap_used` line exempt) before proceeding. Do not commit a failing self-test.

- [ ] **Step 5: Commit**

```bash
git add scripts/check-suppressions.sh scripts/fixtures/suppress-test.rs
git commit -m "ci: add suppression justification gate script"
```

---

## Task 5: Justify or remove suppressions flagged by the gate

**Files:** Discovery-driven — any `.rs` file with a suppression flagged by `check-suppressions.sh`.

**This is a discovery-driven task.** The exact unjustified suppressions only exist when the script runs on the real tree. The per-item decision rule is:

> **For each flagged suppression:** Read `AGENTS.md`'s suppression rule. Add a one-line `//` justification on the same line as the `#[allow]`/`#[expect]`, or on the contiguous comment block immediately above it. If the suppression is not warranted (no good reason for it), **delete it** — the policy says "if you can't articulate a reason, the suppression isn't warranted." After fixing, re-run the script to verify.

- [ ] **Step 1: Run the discovery command**

```bash
bash scripts/check-suppressions.sh 2>&1
```

Expected: The command will FAIL (exit non-zero) listing each unjustified suppression as `path:line: <text>`. Capture the full output.

- [ ] **Step 2: Fix each flagged suppression**

For each line reported:
1. Read the source file at the reported `path:line`.
2. Apply one of these actions:
   - **Add justification:** Add a `//` comment on the same line or on the contiguous comment block above, explaining **why** the suppression is correct. Format: `// <reason>`.
   - **Delete if unwarranted:** If there's no good reason for the suppression, remove the `#[allow]`/`#[expect]` line entirely and fix the underlying issue if needed.
3. After fixing, re-run:

```bash
bash scripts/check-suppressions.sh 2>&1
```

4. Repeat until the command exits 0 (no unjustified suppressions).

- [ ] **Step 3: Verify the full gate still passes**

```bash
bash scripts/check.sh
```

Expected: PASS (all three steps green). If it fails, fix the failure and re-run.

- [ ] **Step 4: Commit**

Stage only the `.rs` files that changed:

```bash
git add -u crates src   # tracked .rs modifications under crates/ and src/ only — covers nested dirs, no untracked sweep, excludes unrelated files like .claude/settings.local.json
git commit -m "chore: justify or remove #[allow] suppressions per gate"
```

> **STOP-AND-REPORT if the suppression count is large enough to balloon this task.** If more than ~15 suppressions need fixing, split this into sub-tasks by crate and report so the plan can be adjusted.

---

## Task 6: Wire suppression gate into `check.sh` and `ci.yml`

**This is a land-then-gate task.** The repo was cleaned in Task 5. Now we wire the gate in. Two commits: the normalize commit (already done in Task 5), then the wire-in commit.

**Files:**
- Modify: `scripts/check.sh` (add the suppression script call)
- Modify: `.github/workflows/ci.yml` (add the suppression step)

- [ ] **Step 1: Add the suppression call to `check.sh`**

Read `scripts/check.sh`. Find the `cargo test` section and add the suppression call before it:

```bash
echo '=== suppression justifications ==='
bash scripts/check-suppressions.sh || failed=1
echo '=== cargo test ==='
cargo test --workspace || failed=1
```

- [ ] **Step 2: Add the suppression step to `ci.yml`**

Read `.github/workflows/ci.yml`. Add a step after `taplo fmt --check` (or after `cargo clippy` if taplo isn't wired yet — the order doesn't matter functionally). The step runs on Linux only since it's a bash script:

```yaml
      - name: Suppression justification gate
        if: runner.os == 'Linux'
        run: bash scripts/check-suppressions.sh
```

- [ ] **Step 3: Verify the full gate passes**

```bash
bash scripts/check.sh
```

Expected: PASS (all steps green, including the new suppression check).

- [ ] **Step 4: Verify YAML is well-formed**

```bash
python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('YAML OK')" 2>/dev/null \
  || python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('YAML OK')" 2>/dev/null \
  || echo "YAML check skipped (no python+pyyaml available) — inspect .github/workflows/ci.yml indentation manually"
```

Expected: `YAML OK`

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/ci.yml scripts/check.sh
git commit -m "ci: wire suppression gate into check.sh and CI"
```

---

## Task 7: Install taplo and normalize all TOML

**This is a land-then-gate task.** Two commits: first normalize all TOML with `taplo fmt`, then add the `--check` gate.

**Decision — taplo default style:** Accept taplo's defaults. Run `taplo fmt` once to normalize all TOML, eyeball the diff. If it's wildly different from the current hand-formatting, add a minimal `taplo.toml` at the repo root to preserve the existing style. Start with defaults.

**Decision — taplo install cost:** `cargo install taplo-cli --locked` is slow. Rely on `Swatinem/rust-cache` (already present in CI) to cache the build. If install time is a problem after landing, switch to a prebuilt-binary action — measure first.

**Decision — guard in `check.sh`:** Add a `command -v taplo` guard so a contributor without taplo gets a clear "install taplo" message rather than a confusing failure. CI always has taplo (no guard needed there).

- [ ] **Step 1: Install taplo locally**

```bash
cargo install taplo-cli --locked
```

Expected: Success (may take a few minutes). Verify:

```bash
taplo --version
```

Expected: `taplo 0.x.x`

- [ ] **Step 2: Run `taplo fmt` to normalize all TOML**

```bash
taplo fmt
```

This normalizes all `.toml` files in the repo (workspace `Cargo.toml`, crate `Cargo.toml`s, `manifest.toml`s, `rust-toolchain.toml`, any `taplo.toml`).

- [ ] **Step 3: Eyeball the diff**

```bash
git diff --stat
```

Review the changes. If the diff is reasonable (key ordering, spacing, array formatting), proceed. If it's wildly different from the current hand-formatting, create a `taplo.toml` at the repo root to adjust:

```toml
# Minimal taplo config to preserve the current hand-formatting style.
# Add keys here only if the default style conflicts with existing formatting.
```

> **Decision point:** If you need a `taplo.toml`, add it in this same commit (the normalize commit). The self-test is: `taplo fmt --check` should exit 0 after the commit.

- [ ] **Step 4: Verify normalization is clean**

```bash
taplo fmt --check
```

Expected: `All checks passed` (exit 0).

- [ ] **Step 5: Verify the full gate still passes**

```bash
bash scripts/check.sh
```

Expected: PASS (all three steps green).

- [ ] **Step 6: Commit the normalize**

Stage only the TOML files taplo changed (and a new `taplo.toml` if you created one).
`Cargo.lock` is not a `.toml` file and taplo does not touch it, so it is excluded — do
not stage it here.

```bash
git add -u '*.toml'                       # tracked .toml modifications, recursive, no untracked sweep
git add taplo.toml 2>/dev/null || true    # include a newly-created taplo.toml if present
git commit -m "chore: normalize TOML with taplo fmt"
```

- [ ] **Step 7: Add taplo install + check to `ci.yml`**

Read `.github/workflows/ci.yml`. Add these steps after `cargo fmt --check` and before `cargo clippy`:

```yaml
      - name: Install taplo
        run: cargo install taplo-cli --locked
      - run: taplo fmt --check
```

- [ ] **Step 8: Add taplo check to `check.sh` with guard**

Read `scripts/check.sh`. Add the taplo section after `cargo fmt`:

```bash
echo '=== taplo fmt ==='
if command -v taplo >/dev/null 2>&1; then
    taplo fmt --check || failed=1
else
    echo "ERROR: taplo not found. Install with: cargo install taplo-cli --locked"
    failed=1
fi
```

- [ ] **Step 9: Verify the full gate passes**

```bash
bash scripts/check.sh
```

Expected: PASS (all steps green, including taplo).

- [ ] **Step 10: Verify YAML is well-formed**

```bash
python -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('YAML OK')" 2>/dev/null \
  || python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('YAML OK')" 2>/dev/null \
  || echo "YAML check skipped (no python+pyyaml available) — inspect .github/workflows/ci.yml indentation manually"
```

Expected: `YAML OK`

- [ ] **Step 11: Commit the wire-in**

```bash
git add .github/workflows/ci.yml scripts/check.sh
git commit -m "ci: add taplo fmt --check gate"
```

---

## Final verification

After all tasks are complete, run the full gate one final time:

```bash
bash scripts/check.sh
```

Expected: PASS (all steps green).

Then review the final state of each modified file:

```bash
cat .github/workflows/ci.yml
cat scripts/check.sh
cat scripts/check-suppressions.sh
```

Verify:
- [ ] CI `env` has `RUSTFLAGS: "-Dwarnings"`
- [ ] CI `test` job has `timeout-minutes: 30` and `concurrency`
- [ ] CI has `cargo test --workspace --release` step with explanatory name
- [ ] CI has `cargo install taplo-cli --locked` and `taplo fmt --check`
- [ ] CI has suppression step with `if: runner.os == 'Linux'`
- [ ] CI has `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] CI has `cargo test --workspace`
- [ ] `check.sh` has `RUSTFLAGS` export
- [ ] `check.sh` has suppression script call
- [ ] `check.sh` has taplo check with `command -v` guard
- [ ] `check.sh` has release-test divergence comment
- [ ] `scripts/check-suppressions.sh` is executable and runs clean
- [ ] `.claude/settings.json` Stop hook is unchanged (still runs the three original commands)

---

## Summary of commits (expected)

| # | Commit message | Files |
|---|---|---|
| 1 | `ci: enable RUSTFLAGS=-Dwarnings in CI and check.sh` | `ci.yml`, `check.sh` |
| 2 | `fix: resolve rustc warnings under RUSTFLAGS=-Dwarnings` | discovery-driven `.rs` files |
| 3 | `ci: add release-profile test step (CI-only)` | `ci.yml`, `check.sh` |
| 4 | `ci: add suppression justification gate script` | `check-suppressions.sh`, `fixtures/suppress-test.rs` |
| 5 | `chore: justify or remove #[allow] suppressions per gate` | discovery-driven `.rs` files |
| 6 | `ci: wire suppression gate into check.sh and CI` | `ci.yml`, `check.sh` |
| 7 | `chore: normalize TOML with taplo fmt` | discovery-driven `.toml` files |
| 8 | `ci: add taplo fmt --check gate` | `ci.yml`, `check.sh` |
