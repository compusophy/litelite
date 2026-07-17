#!/usr/bin/env bash
# Auto-restart wrapper for the prooflite fine-tune.
#
# The trainer checkpoints every round and resumes from the latest C<n>, so a
# crash costs at most one round of recompute. On 24GB the run lives at the
# memory edge: even with gradient checkpointing + expandable_segments a residual
# fragmentation OOM can still fell the process at a round boundary. This loop
# makes that self-healing — it relaunches until a target checkpoint exists (or
# the trainer exits cleanly), so the whole run is fire-and-forget overnight.
#
# A fresh process resets the CUDA allocator, so a restart clears exactly the
# fragmentation that killed the prior one. If the trainer ever crashes BEFORE
# saving a new checkpoint on two consecutive attempts, we're stuck in a real
# (not transient) OOM and the loop bails rather than spinning forever.
set -u
cd "$(dirname "$0")"

TARGET=11          # C11 = 12 rounds (0..11); matches the original plan
MAX_ATTEMPTS=40

latest() {  # highest C<n> index, or -1 if none
  ls -1 checkpoints_prooflite 2>/dev/null \
    | sed -n 's/^C\([0-9]\+\)$/\1/p' | sort -n | tail -1 | sed 's/^$/-1/'
}

prev=-1
stuck=0
for attempt in $(seq 1 "$MAX_ATTEMPTS"); do
  cur=$(latest)
  if [ "${cur:--1}" -ge "$TARGET" ]; then
    echo "run_loop: reached C${cur} >= C${TARGET} — done (attempt ${attempt})"
    break
  fi

  # No forward progress since the last attempt → a hard, non-transient failure.
  if [ "$attempt" -gt 1 ] && [ "${cur:--1}" -le "$prev" ]; then
    stuck=$((stuck + 1))
    if [ "$stuck" -ge 2 ]; then
      echo "run_loop: no progress past C${cur} in 2 attempts — bailing (not transient)"
      exit 1
    fi
  else
    stuck=0
  fi
  prev=${cur:--1}

  echo "run_loop: launching trainer (attempt ${attempt}, latest=C${cur}) $(date)"
  ./.venv/Scripts/python.exe run_prooflite.py
  code=$?
  echo "run_loop: trainer exited code=${code} $(date)"
  [ "$code" -eq 0 ] && { echo "run_loop: clean exit"; break; }
  sleep 5
done
