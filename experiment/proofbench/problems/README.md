# experiment/proofbench/problems — the transfer / problem-solving benchmark

`bench.py` + `p6 eval` measure whether the model generates *well-formed* prooflite.
This measures something stronger and less expected: can it **solve a specified
problem** — produce a program whose OUTPUT is correct, not merely valid?

`heldout.jsonl` is 18 held-out problems, each `{id, spec, ref}`:
- `spec` — a task, phrased to complete "a program that {spec}" (the trainer's
  prompt shape), so the generation prompt is in-distribution.
- `ref` — a verified reference solution. Its output is the ground truth; a
  candidate SOLVES the problem iff its output equals the reference's (exact match,
  trailing whitespace normalized). Every ref self-solves at 100%.

The problems are deliberately **not** in the training styles or corpus: specific
sequences, accumulations, branching, integer arithmetic, and nested loops with one
correct answer each.

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
