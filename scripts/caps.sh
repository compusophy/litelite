#!/usr/bin/env bash
# The constitution's teeth: LOC caps per crate + repo, char cap on CLAUDE.md.
# At a cap: split, shrink, or kill — never raise the cap.
set -uo pipefail
cd "$(dirname "$0")/.."

CRATE_CAP=2000
REPO_CAP=25000
CLAUDE_CAP=8000
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

# Normalize CRLF so the cap measures canonical bytes on Windows checkouts too.
chars=$(tr -d '\r' <CLAUDE.md | wc -c)
printf '%-22s %6d chars (cap %d)\n' "CLAUDE.md" "$chars" "$CLAUDE_CAP"
if [ "$chars" -gt "$CLAUDE_CAP" ]; then
  echo "FAIL: CLAUDE.md exceeds the surface cap"
  fail=1
fi

if grep -rn -- '[-]lite' crates src scripts paper .github ./*.md ./*.toml; then
  echo "FAIL: dashed lite reference found (constitution rule 7)"
  fail=1
fi

exit "$fail"
