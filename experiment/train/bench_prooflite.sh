#!/usr/bin/env bash
# Generate the N=2 (prooflite) benchmark pools: identical prompts + sampling for
# the base model and the checkpoints bracketing the C6 diversity peak, so the
# peak can be confirmed on FRESH samples (not just training-time histograms).
# Each pool is 256 programs (32 per style). Scored afterward by `p6 eval`.
set -eu
cd "$(dirname "$0")"

export BENCH_BIN="C:/Users/kyle/Downloads/apply/litelite/experiment/proofbench/target/release/p6.exe"
export PYTORCH_CUDA_ALLOC_CONF="max_split_size_mb:256"
PY=./.venv/Scripts/python.exe
OUT=../proofbench/results
mkdir -p "$OUT"

# Headline pair (c6 peak, base floor) first, then the selection-curve neighbours.
gen() { echo "=== $1  <- $2  $(date) ==="; "$PY" bench.py "$2" "$OUT/$1.jsonl" 32; }
gen c6   checkpoints_prooflite/C6
gen base Qwen/Qwen3-0.6B
gen c5   checkpoints_prooflite/C5
gen c7   checkpoints_prooflite/C7
gen c8   checkpoints_prooflite/C8
echo "=== ALL POOLS DONE $(date) ==="
