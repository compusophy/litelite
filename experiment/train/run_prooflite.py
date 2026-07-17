"""Launch the N=2 prooflite fine-tune. The language-parametric trainer runs
UNCHANGED — this only points it at the p6 reward binary and the prooflite
corpus, and turns off the candle requirement (prooflite has no market data).

    python run_prooflite.py

Everything else — cold start on the corpus survivors, verifier-only
rejection-sampling self-play, checkpoints, the anti-collapse guards — is the
same code path that produced the stratlite result.
"""

from __future__ import annotations

import os

from train import Config, run

HERE = os.path.dirname(os.path.abspath(__file__))
P6 = os.path.join(HERE, "..", "proofbench", "target", "release", "p6.exe")
CORPUS = os.path.join(HERE, "..", "proofbench", "corpus", "seed.jsonl")

if __name__ == "__main__":
    run(
        Config(
            s5_bin=P6,
            corpus=CORPUS,
            candles=(),  # prooflite reward is data-free
            needs_candles=False,
            out_dir="checkpoints_prooflite",
        )
    )
