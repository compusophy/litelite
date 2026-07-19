"""Launch the SPEC-CONDITIONED self-play arm — the fix that the transfer
benchmark's negative half implies (paper §5.8). Same trainer, same admission
guards, same rejection-sampling loop; two swaps only:

  * every sampling prompt is a training TASK ("a program that {spec}",
    `p6 trainstyles problems/train.jsonl` — 48 specs disjoint from the 30
    held-out benchmark problems, no family overlap with the held-out hard tier);
  * the reward's top rung is CORRECTNESS — output equal to that spec's
    reference — not RICH shape (`p6 solvereward`).

Starts from the committed-recipe Cinit (plain SFT on the corpus survivors),
exactly where the unconditional self-play arm started, so C6 and this arm
differ ONLY in what the verifier rewarded.

    python run_solve.py        # resumes from the latest checkpoints_solve/C<n>
"""

from __future__ import annotations

import os

os.environ.setdefault(
    "PYTORCH_CUDA_ALLOC_CONF", "max_split_size_mb:256,garbage_collection_threshold:0.8"
)

from train import Config, run

HERE = os.path.dirname(os.path.abspath(__file__))
P6 = os.path.join(HERE, "..", "proofbench", "target", "release", "p6.exe")
PROBLEMS = os.path.join(HERE, "..", "proofbench", "problems", "train.jsonl")
CINIT = os.path.join(HERE, "checkpoints_prooflite", "Cinit")
OUT = "checkpoints_solve"


def latest_checkpoint() -> tuple[str, int] | None:
    if not os.path.isdir(OUT):
        return None
    ns = [int(d[1:]) for d in os.listdir(OUT) if d.startswith("C") and d[1:].isdigit()]
    return (os.path.join(OUT, f"C{max(ns)}"), max(ns)) if ns else None


if __name__ == "__main__":
    base = Config(
        s5_bin=P6,
        lang="prooflite",
        candles=(),
        needs_candles=False,
        out_dir=OUT,
        base_model=CINIT,  # no cold start here: Cinit IS the cold start
        corpus="",
        styles_args=("trainstyles", PROBLEMS),
        reward_args=("solvereward", PROBLEMS),
        # 48 spec-prompts x 16 = 768 samples/round (the unconditional arm's
        # 8 styles x 128 = 1024); same batch sizing as run_prooflite.
        samples_per_style=16,
        rounds=6,
        sft_batch=4,
    )
    ckpt = latest_checkpoint()
    if ckpt is not None:
        path, n = ckpt
        base.base_model = path
        base.round_offset = n + 1
        base.rounds = max(0, 6 - (n + 1))
        print(f"resuming from {path} (next round {n + 1})", flush=True)
    run(base)
