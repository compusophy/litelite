#!/usr/bin/env bash
# Publish every kit crate to crates.io.
#
#   scripts/publish.sh              # DRY RUN (the default — publishing is forever)
#   scripts/publish.sh --execute    # the real upload
#
# Cargo owns the dependency ORDER: `--workspace` topologically sorts the
# members and waits for each to index before the next one needs it. This
# script never hard-codes a crate list — it asks cargo — so adding a crate
# cannot drift it.
#
# What it adds is RESUMABILITY. crates.io rate-limits NEW crates (a small
# burst, then roughly one per ten minutes), so the first publish of a kit this
# size can stop partway. Re-running is then safe: versions already live are
# excluded and cargo publishes only the rest.
set -uo pipefail
cd "$(dirname "$0")/.."

execute=0
[ "${1:-}" = "--execute" ] && execute=1

command -v curl >/dev/null || { echo "FAIL: curl is required"; exit 1; }

# The crate list and the version, both straight from cargo.
members=$(cargo tree --workspace --depth 0 2>/dev/null | awk '/^[[:alnum:]]/ {print $1}' | sort -u)
version=$(cargo tree --workspace --depth 0 2>/dev/null |
  awk '$1 == "litelite" {print substr($2, 2); exit}')
[ -n "$members" ] && [ -n "$version" ] || { echo "FAIL: cargo told us nothing"; exit 1; }
echo "litelite $version — $(wc -w <<<"$members") crates"

# Who we expect to own these names, derived from the repository field so it
# cannot drift from the manifest.
repo=$(grep -m1 '^repository = ' Cargo.toml | cut -d'"' -f2)
owner=$(basename "$(dirname "$repo")")
[ -n "$owner" ] || { echo "FAIL: no repository owner in Cargo.toml"; exit 1; }

# Exclude what is already live. Order-independent, so it cannot disagree with
# cargo's ordering; it only ever removes work. (crates.io 403s any request
# without an identifying User-Agent — its documented policy.)
ua="litelite-publish ($repo)"
get() { curl -s --max-time 20 --retry 3 --retry-connrefused -A "$ua" "$@"; }

excludes=()
pending=0
for name in $members; do
  code=$(get -o /dev/null -w '%{http_code}' \
    "https://crates.io/api/v1/crates/$name/$version")
  case "$code" in
    200)
      # 200 only means the name@version EXISTS — it says nothing about who
      # published it. Excluding a squatted name would report someone else's
      # crate as our success, so prove it is ours before skipping it.
      if get "https://crates.io/api/v1/crates/$name/owners" |
        grep -qi "\"login\"[[:space:]]*:[[:space:]]*\"$owner\""; then
        printf '  %-14s already ours on crates.io\n' "$name"
        excludes+=(--exclude "$name")
      else
        echo "FAIL: $name $version exists on crates.io and $owner does not own it"
        exit 1
      fi
      ;;
    404)
      printf '  %-14s to publish\n' "$name"
      pending=$((pending + 1))
      ;;
    *)
      echo "FAIL: crates.io returned $code for $name $version"
      exit 1
      ;;
  esac
done

if [ "$pending" -eq 0 ]; then
  echo "nothing to do: every crate is already ours at $version (bump it to ship)"
  exit 0
fi

if [ "$execute" -eq 1 ]; then
  echo "publishing $pending crate(s) — this is NOT a drill"
  cargo publish --workspace "${excludes[@]}"
else
  echo "dry run of $pending crate(s); pass --execute to upload"
  cargo publish --workspace --dry-run "${excludes[@]}"
fi
