#!/usr/bin/env bash
# The constitution's teeth: LOC caps per crate + repo, char cap on CLAUDE.md.
# At a cap: split, shrink, or kill — never raise the cap.
set -uo pipefail
cd "$(dirname "$0")/.."

CRATE_CAP=2000
REPO_CAP=25000
CLAUDE_CAP=8000
# experiment/ is outside the kit's WORKSPACE (it takes deps; rules 1+5 keep
# them out of the graph) — but not outside the constitution. Without a counter
# here the seam is a hole the caps cannot see, and "the cap can't count it"
# is exactly the Goodhart failure the paper's §6 lists as a known limit.
EXP_CAP=1500
# The M6 trainer (experiment/train/*.py) is thin BY DESIGN — the guards are
# the load-bearing part and the torch loop is a shell. Its own counter, because
# the constitution's discipline does not stop at the language boundary.
TRAIN_CAP=800
fail=0

for c in crates/*/; do
  n=$(find "$c" -name '*.rs' -print0 | xargs -0 cat | wc -l)
  printf '%-22s %6d LOC (cap %d)\n' "$c" "$n" "$CRATE_CAP"
  if [ "$n" -gt "$CRATE_CAP" ]; then
    echo "FAIL: $c exceeds the per-crate cap"
    fail=1
  fi
done

total=$(find crates src -name '*.rs' -print0 | xargs -0 cat | wc -l)
printf '%-22s %6d LOC (cap %d)\n' "repo total" "$total" "$REPO_CAP"
if [ "$total" -gt "$REPO_CAP" ]; then
  echo "FAIL: repo exceeds the total cap"
  fail=1
fi

if [ -d experiment/src ]; then
  n=$(find experiment/src -name '*.rs' -print0 | xargs -0 cat | wc -l)
  printf '%-22s %6d LOC (cap %d)\n' "experiment/" "$n" "$EXP_CAP"
  if [ "$n" -gt "$EXP_CAP" ]; then
    echo "FAIL: experiment/ exceeds its cap — the harness is thin BY DESIGN"
    fail=1
  fi
fi

for bench in proofbench appbench; do
  if [ -d "experiment/$bench/src" ]; then
    n=$(find "experiment/$bench/src" -name '*.rs' -print0 | xargs -0 cat | wc -l)
    printf '%-22s %6d LOC (cap %d)\n' "experiment/$bench/" "$n" "$EXP_CAP"
    if [ "$n" -gt "$EXP_CAP" ]; then
      echo "FAIL: experiment/$bench/ exceeds its cap"
      fail=1
    fi
  fi
done

if [ -d app/src ]; then
  # The vibe-coding shell is a THIN boundary by design: applite does the
  # work; the shell is one ABI file + one page. index.html counts too — UI
  # bloat is still bloat.
  n=$(find app/src -name '*.rs' -print0 | xargs -0 cat | wc -l)
  h=$(tr -d '\r' <app/index.html | wc -l)
  p=$(find app -maxdepth 1 -name '*.py' -print0 | xargs -0 cat | wc -l)
  printf '%-22s %6d LOC (cap %d)  + index.html %d (cap %d) + py %d (cap %d)\n' "app/" "$n" 500 "$h" 400 "$p" 150
  if [ "$n" -gt 500 ] || [ "$h" -gt 400 ] || [ "$p" -gt 150 ]; then
    echo "FAIL: app/ exceeds its cap — the shell is a boundary, not a home"
    fail=1
  fi
fi

if [ -d experiment/train ]; then
  # -maxdepth 1: the gitignored .venv/ holds 100K+ lines of site-packages —
  # the caps measure the repo, never the toolchain.
  n=$(find experiment/train -maxdepth 1 -name '*.py' -print0 | xargs -0 cat | wc -l)
  printf '%-22s %6d LOC (cap %d)\n' "experiment/train/" "$n" "$TRAIN_CAP"
  if [ "$n" -gt "$TRAIN_CAP" ]; then
    echo "FAIL: experiment/train/ exceeds its cap — the trainer is a shell BY DESIGN"
    fail=1
  fi
fi

# Normalize CRLF so the cap measures canonical bytes on Windows checkouts too.
chars=$(tr -d '\r' <CLAUDE.md | wc -c)
printf '%-22s %6d chars (cap %d)\n' "CLAUDE.md" "$chars" "$CLAUDE_CAP"
if [ "$chars" -gt "$CLAUDE_CAP" ]; then
  echo "FAIL: CLAUDE.md exceeds the surface cap"
  fail=1
fi

if grep -rn -- '[-]lite' crates src scripts paper .github app/src app/*.md app/*.html app/*.toml experiment/src experiment/proofbench/src experiment/train/*.py experiment/train/*.md experiment/train/*.txt experiment/corpus experiment/results/*.md experiment/results/*.txt experiment/*.toml experiment/*.md ./*.md ./*.toml; then
  echo "FAIL: dashed lite reference found (constitution rule 7)"
  fail=1
fi

exit "$fail"
