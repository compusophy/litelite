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

if [ -d experiment/train ]; then
  n=$(find experiment/train -name '*.py' -print0 | xargs -0 cat | wc -l)
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

if grep -rn -- '[-]lite' crates src scripts paper .github experiment/src experiment/train experiment/*.toml experiment/*.md ./*.md ./*.toml; then
  echo "FAIL: dashed lite reference found (constitution rule 7)"
  fail=1
fi

exit "$fail"
