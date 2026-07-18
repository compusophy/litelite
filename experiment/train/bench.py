"""Sample a benchmark pool from a model (base or checkpoint) for `s5 eval`.

    python bench.py <model_id_or_checkpoint_path> <out.jsonl> [k_per_style]

The benchmark protocol (pre-registered in ../M6.md): identical prompts and
sampling parameters for every model; the pool is scored by the deterministic
verifier on three windows — the train month (in-distribution), a distant month
(chronological embargo), and a second asset (cross-asset). Report compile rate
and the CONDITIONAL gate-clear rate per window; the base model's column is the
baseline the fine-tune must beat.
"""

from __future__ import annotations

import json
import os
import sys

from train import Config, Policy, card, styles
from admission import extract_source


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    model, out = sys.argv[1], sys.argv[2]
    k = int(sys.argv[3]) if len(sys.argv) > 3 else 32
    # BENCH_BIN points the card/styles source at a different language's binary
    # (p6 for the N=2 prooflite arm); unset keeps the default stratlite oracle,
    # so the committed stratlite pools reproduce unchanged. Generation never
    # scores, so no candles are needed here regardless of language.
    bin_override = os.environ.get("BENCH_BIN")
    cfg = Config(base_model=model, **({"s5_bin": bin_override} if bin_override else {}))
    policy = Policy(cfg, card(cfg))
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        for si, style in enumerate(styles(cfg)):
            for j, completion in enumerate(policy.sample(style, k)):
                row = {
                    "id": f"s{si}n{j}",
                    # The style INDEX, not the prompt's second word — every style
                    # begins "a program/strategy ...", so word[1] is not unique.
                    "style": f"s{si}",
                    "source": extract_source(completion),
                }
                f.write(json.dumps(row) + "\n")
    print(f"{out}: {8 * k} samples from {model}")


if __name__ == "__main__":
    main()
