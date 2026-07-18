#!/usr/bin/env bash
# Run the transfer / problem-solving benchmark across models, one command.
# Generation is GPU (solve_bench.py); scoring is CPU (p6 solve). Produces the
# base vs Cinit (plain-SFT) vs C6 (self-play) pass@k-per-tier comparison — the
# transfer result AND the Direction-2 baselines from one harness.
#
#   ./run_transfer.sh [k]        # k = samples per problem (default 8)
set -eu
cd "$(dirname "$0")"   # -> experiment/train

export BENCH_BIN="../proofbench/target/release/p6.exe"   # relative; resolves from here
export PYTORCH_CUDA_ALLOC_CONF="max_split_size_mb:256"
PY=./.venv/Scripts/python.exe
P6=../proofbench/target/release/p6.exe
PROB=../proofbench/problems/heldout.jsonl
OUT=../proofbench/results
K=${1:-8}
mkdir -p "$OUT"

# name -> model/checkpoint. base = the pretrained floor; cinit = cold-start
# (plain SFT, no self-play); c6 = the selected self-play checkpoint.
gen() { echo "=== generate $1 <- $2  $(date) ==="; "$PY" solve_bench.py "$2" "$PROB" "$OUT/solve_$1.jsonl" "$K"; }
gen base  Qwen/Qwen3-0.6B
gen cinit checkpoints_prooflite/Cinit
gen c6    checkpoints_prooflite/C6

echo ""
echo "########## TRANSFER RESULTS (pass@${K}) ##########"
for name in base cinit c6; do
  echo "----- $name -----"
  "$P6" solve "$PROB" "$OUT/solve_$name.jsonl" | head -5
  echo
done
