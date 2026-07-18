# experiment/proofbench/problems — the transfer / problem-solving benchmark

`bench.py` + `p6 eval` measure whether the model generates *well-formed* prooflite.
This measures something stronger and less expected: can it **solve a specified
problem** — produce a program whose OUTPUT is correct, not merely valid?

`heldout.jsonl` is 30 held-out problems, each `{id, tier, spec, ref}`:
- `tier` — `easy` (9) / `medium` (9) / `hard` (12); `p6 solve` reports pass@k per
  tier, so the result shows a competence gradient, not one blended number.
- `spec` — a task, phrased to complete "a program that {spec}" (the trainer's
  prompt shape), so the generation prompt is in-distribution.
- `ref` — a verified reference solution. Its output is the ground truth; a
  candidate SOLVES the problem iff its output equals the reference's (exact match,
  trailing whitespace normalized). Every ref self-solves at 100%, and each ref's
  output was checked with `p6 run <program>` to equal the intended answer.

The problems are deliberately **not** in the training styles or corpus, and the
`hard` tier needs real algorithms with a single correct answer: primality
(`is_prime`), popcount, digit-sum, Collatz length, alternating sums, powers — the
discriminators where a base model fails and only genuine competence succeeds.

## Run it

    # 1. generate solutions (GPU) for each model under test:
    cd experiment/train
    BENCH_BIN=<abs path to p6.exe> ./.venv/Scripts/python.exe \
        solve_bench.py <model_or_ckpt> ../proofbench/problems/heldout.jsonl sols.jsonl 8

    # 2. score pass@k (CPU):
    cd experiment/proofbench
    ./target/release/p6 solve problems/heldout.jsonl ../train/sols.jsonl

Run it on **base Qwen3-0.6B**, **Cinit** (cold-start = plain SFT, no self-play),
and **C6** (verifier-only self-play). That single harness yields both:
- the **transfer** result (does verified generation competence become
  problem-SOLVING competence? base vs C6), and
- the **Direction-2 baselines** (is the *self-play* what does it? Cinit vs C6; is
  the *verifier* what does it? add a no-verifier run).
