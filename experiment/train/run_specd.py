"""The SPEC-CONDITIONED self-play launcher, parametric over the reward
binary — one runner for every language arm (§5.8's fix, generalized). Every
sampling prompt is a task (`<bin> trainstyles <tasks>`), the reward's top
rung is that task's own check (`<bin> solvereward <tasks>` — exact output
for prooflite's p6, the behavioral event script for applite's a8).

    python run_specd.py <reward_bin> <tasks.jsonl> <out_dir> \
        [--base MODEL_OR_CKPT] [--corpus SEED.jsonl] [--rounds N] [--k N] [--lang L]

The prooflite arm (results committed 2026-07-19):
    python run_specd.py ../proofbench/target/release/p6.exe \
        ../proofbench/problems/train.jsonl checkpoints_solve \
        --base checkpoints_prooflite/Cinit --lang prooflite
The applite arm (cold-starts on the behavioral corpus):
    python run_specd.py ../appbench/target/release/a8.exe \
        ../appbench/tasks/train.jsonl checkpoints_apps \
        --corpus ../appbench/corpus/seed.jsonl --lang applite

Resumes from the highest C<n> in <out_dir> (cold start is skipped then).
"""

from __future__ import annotations

import os
import sys

os.environ.setdefault(
    "PYTORCH_CUDA_ALLOC_CONF", "max_split_size_mb:256,garbage_collection_threshold:0.8"
)

from train import Config, run


def latest_checkpoint(out_dir: str) -> tuple[str, int] | None:
    if not os.path.isdir(out_dir):
        return None
    ns = [int(d[1:]) for d in os.listdir(out_dir) if d.startswith("C") and d[1:].isdigit()]
    return (os.path.join(out_dir, f"C{max(ns)}"), max(ns)) if ns else None


def arg(flag: str, default: str) -> str:
    return sys.argv[sys.argv.index(flag) + 1] if flag in sys.argv else default


if __name__ == "__main__":
    if len(sys.argv) < 4:
        raise SystemExit(__doc__)
    bin_path, tasks, out_dir = sys.argv[1], sys.argv[2], sys.argv[3]
    rounds = int(arg("--rounds", "6"))
    cfg = Config(
        s5_bin=bin_path,
        lang=arg("--lang", "applite"),
        candles=(),
        needs_candles=False,
        out_dir=out_dir,
        base_model=arg("--base", Config().base_model),
        corpus=arg("--corpus", ""),
        styles_args=("trainstyles", tasks),
        reward_args=("solvereward", tasks),
        samples_per_style=int(arg("--k", "16")),
        rounds=rounds,
        sft_batch=4,
    )
    ckpt = latest_checkpoint(out_dir)
    if ckpt is not None:
        path, n = ckpt
        cfg.base_model = path
        cfg.corpus = ""  # past cold start by definition
        cfg.round_offset = n + 1
        cfg.rounds = max(0, rounds - (n + 1))
        print(f"resuming from {path} (next round {n + 1})", flush=True)
    run(cfg)
