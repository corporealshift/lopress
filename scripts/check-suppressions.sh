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
