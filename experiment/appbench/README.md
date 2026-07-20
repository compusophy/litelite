# appbench — a local model that writes working apps, verified behaviorally

The generator arm of M7: teach Qwen3-0.6B (running on one consumer GPU, no
API, no teacher) to write `applite` apps that DO WHAT WAS ASKED — not merely
compile. The reward tool `a8` speaks the same CLI protocol as s5/p6, so the
language-parametric trainer ran unchanged; what is new is the top rung:

- a task is a spec plus an EVENT SCRIPT — clicks by button label, typing
  into inputs by position, and `assert_shows` / `assert_exact` /
  `assert_hides` assertions over the rendered labels;
- the ladder is compile (0) → run-fault (1/3) → wrong-behavior (2/3) →
  **script passes (1)**. A valid app that does the wrong thing GATES.

This is §5.8's lesson built in from day one — the reward is the spec, never
an output shape — and the instrument caught its own first bug to prove the
point: "shows 1" by substring also accepted a decrementing counter's "-1";
`assert_exact` exists because of it, and a test pins it.

## The run (2026-07-19, ~64 minutes end to end on a 3090)

Cold start: SFT on 120 corpus programs — 3 behavior-preserving variants
(verbatim / state-renamed / col-wrapped) of the 40 training-task references,
every one validated against its script before it may teach. Then 6 rounds of
spec-conditioned rejection-sampling self-play (16 samples per task per
round). Correct-of-640 on the training tasks:

    cold start -> R0 83.3% -> R1 91.7% -> R2 94.8% -> R3 95.5% -> R4 98.1% -> R5 98.8%

## The result: 16 held-out tasks, 8 attempts each

| model | behavioral pass@8 | per-attempt ladder (of 128) |
|---|---|---|
| base Qwen3-0.6B | **0 / 16 (0%)** | 128 compile-fail |
| C5 (cold start + self-play) | **16 / 16 (100%)** | 107 ok, 18 gate, 3 compile |

The held-out tasks share no id with training and are fresh combinations
(elevator floors with a min-clamp, like/dislike tallies, a dark-mode toggle,
`n -> 2n+1` steppers, a 3-click unlock, an input with a clear button…).
The floor is genuinely zero: applite did not exist when the base model was
trained, and all 128 of its attempts fail to compile. After one hour of
verifier-only training, the model writes a working app for every held-out
spec within 8 tries — and 84% of individual attempts pass their script
outright.

Reproduce the SCORING (the model, as ever, is not reproducible):

    cd experiment/appbench && cargo build --release
    ./target/release/a8 eval tasks/heldout.jsonl results/solve_c5.jsonl
    ./target/release/a8 eval tasks/heldout.jsonl results/solve_base.jsonl

Regenerate pools (GPU): `cd ../train && BENCH_BIN=../appbench/target/release/a8.exe
PREFIX="an app that" ./.venv/Scripts/python.exe solve_bench.py
checkpoints_apps/C5 ../appbench/tasks/heldout.jsonl out.jsonl 8`. Train from
scratch: `run_specd.py` (see its docstring).

## What this closes, and honest bounds

This closes the M7 loop end to end, keyless: describe an app → a LOCAL
fine-tune writes it → `applite` proves it total (halts, atomic faults,
bounded memory) → `a8`-style scripts can prove it BEHAVES → the `app/`
shell runs it live. Bounds to keep: 16 tasks is a small benchmark and its
interaction grammar deliberately matches training's (counters, toggles,
tallies, inputs — fresh combos, not fresh genres); specs name button labels
and displayed strings explicitly, and the model is trained to satisfy specs
literally — free-form product asks are the shell's copy-prompt path, not
this model's. The gate rung (18/128) is mostly near-misses on wording or
off-by-one behavior; nothing here certifies apps ANYONE would want — that
judgment stays with the human clicking verify.
