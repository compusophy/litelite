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
OUT = "checkpoints_prooflite"


def latest_checkpoint() -> tuple[str, int] | None:
    """The highest-numbered C<n> in OUT, if any — to resume from."""
    if not os.path.isdir(OUT):
        return None
    ns = [int(d[1:]) for d in os.listdir(OUT) if d.startswith("C") and d[1:].isdigit()]
    return (os.path.join(OUT, f"C{max(ns)}"), max(ns)) if ns else None


if __name__ == "__main__":
    base = Config(
        s5_bin=P6, candles=(), needs_candles=False, out_dir=OUT, base_model=Config().base_model
    )
    ckpt = latest_checkpoint()
    if ckpt is not None:
        # Resume: start from the saved checkpoint, skip cold start (already
        # done), and continue checkpoint numbering past it.
        path, n = ckpt
        print(f"resuming from {path} (continuing at round {n + 1})", flush=True)
        run(Config(**{**base.__dict__, "base_model": path, "corpus": "", "round_offset": n + 1}))
    else:
        # Fresh: cold start on the corpus, then self-play from round 0.
        run(Config(**{**base.__dict__, "corpus": CORPUS}))
