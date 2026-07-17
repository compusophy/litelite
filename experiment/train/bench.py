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
import sys

from train import Config, Policy, card, styles
from admission import extract_source


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    model, out = sys.argv[1], sys.argv[2]
    k = int(sys.argv[3]) if len(sys.argv) > 3 else 32
    cfg = Config(base_model=model)
    policy = Policy(cfg, card(cfg))
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        for si, style in enumerate(styles(cfg)):
            for j, completion in enumerate(policy.sample(style, k)):
                row = {
                    "id": f"s{si}n{j}",
                    "style": style.split()[1],  # one-word label for the table
                    "source": extract_source(completion),
                }
                f.write(json.dumps(row) + "\n")
    print(f"{out}: {8 * k} samples from {model}")


if __name__ == "__main__":
    main()
