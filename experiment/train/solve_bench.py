"""Generate held-out problem SOLUTIONS from a model — the transfer / problem-solving
benchmark. Contrast with bench.py (which samples free-form valid programs): here the
model must produce a program whose OUTPUT satisfies a specific spec.

    BENCH_BIN=<p6 abspath> python solve_bench.py <model_or_ckpt> <problems.jsonl> <out.jsonl> [k]

For each problem it prompts the model in the trainer's exact shape — the language
card as the system turn, "a program that {spec}" as the user instruction — and
samples k candidates, writing {id, source} per attempt. Score with
`p6 solve <problems.jsonl> <out.jsonl>` (pass@k by exact output match vs the
reference). Run base, Cinit (plain-SFT), and C6 (self-play) through this to get
both the transfer result and the Direction-2 baselines from one harness.
"""

from __future__ import annotations

import json
import os
import sys

from train import Config, Policy, card
from admission import extract_source


def main() -> None:
    if len(sys.argv) < 4:
        raise SystemExit(__doc__)
    model, problems_path, out = sys.argv[1], sys.argv[2], sys.argv[3]
    k = int(sys.argv[4]) if len(sys.argv) > 4 else 8
    bin_override = os.environ.get("BENCH_BIN")
    cfg = Config(base_model=model, lang="prooflite",
                 **({"s5_bin": bin_override} if bin_override else {}))
    policy = Policy(cfg, card(cfg))
    problems = [json.loads(l) for l in open(problems_path, encoding="utf-8") if l.strip()]
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        for p in problems:
            # The exact _prompt shape the model was trained on, now with a
            # SPECIFIC task. PREFIX env: "a program that" (default) / "an app that".
            prefix = os.environ.get("PREFIX", "a program that")
            for completion in policy.sample(f"{prefix} {p['spec']}", k):
                f.write(json.dumps({"id": p["id"], "source": extract_source(completion)}) + "\n")
    print(f"{out}: {len(problems) * k} solutions ({len(problems)} problems x {k}) from {model}")


if __name__ == "__main__":
    main()
