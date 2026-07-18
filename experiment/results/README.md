# M6 result — verifier-only fine-tuning lifts a small model from 0 to competent

The pre-registered §5 benchmark, RUN. A small open-weights model
(Qwen3-0.6B) was fine-tuned to generate stratlite using only the kit's
verifier as supervision — no teacher, no API key. It is then benchmarked
against the same model before fine-tuning, with identical prompts, scored by
the deterministic verifier on three windows.

## What reproduces, and what does not

The MODEL is not reproducible (sampling is stochastic; a fine-tune is not
bit-identical across hardware). What reproduces is the SCORING: the pools
(`base.jsonl`, `c7.jsonl` — 256 samples each, 32 per style) and the candles
are committed/pinned, and

    cd experiment && cargo build --release   # builds s5 (target/ is git-ignored)
    ./target/release/s5 eval results/{base,c7}.jsonl data/<window>.csv

reproduces every number below (full output in `benchmark.txt`).

## The numbers

Conditional metric: compile rate over the pool, then — among compilers — the
held-out gate-clear rate (a valid, actively-trading strategy).

| window | base compile / gate-clear | C7 compile / gate-clear |
|---|---|---|
| BTCUSDT Jan 2024 (train reward window) | 0.0% / 0.0% | 100.0% / 95.7% |
| BTCUSDT Jun 2024 (distant, 5-month embargo) | 0.0% / 0.0% | 100.0% / 96.5% |
| ETHUSDT Jun 2024 (cross-asset, never trained on) | 0.0% / 0.0% | 100.0% / 96.1% |

## Why the baseline is a true floor, not a weak one

stratlite exists in no pretraining corpus. Base Qwen3-0.6B produces ZERO
valid programs across 256 attempts on every window — it parrots the grammar
card's notation (`signal long|short|flat;` copied literally) but cannot emit
one compiling program. So the lift is entirely the fine-tune, with nothing to
confound it. This is the tempo-x402 result (compiler-verified self-play, 0.5B
model, 1.5% -> 16.4% on a Rust benchmark) generalized off rustc onto a
purpose-built language, from a stronger (measured-0%) floor.

## The training curve (train reward window; `train_curve.log`)

Cold start SFT on the 132 committed corpus survivors, then verifier-only
rejection-sampling self-play. Full-survivor rate per round:

    cold start -> R0 39.0% -> R1 67.0% -> R2 80.6% -> R3 85.8%
    -> R4 89.6% -> R5 94.6% -> R6 96.0% -> R7 98.2%

Stopped at C7 on the protocol's train-saturation early-stop. `distinct_nkeys`
stayed high throughout (392 -> 624 -> 685 ... of ~700-1000 admitted): the
anti-collapse admission guards held; the model learned a diverse grammar, not
one template.

## What this establishes — and what it does not

ESTABLISHES: verifier-only fine-tuning takes a small model from no competence
to ~96% valid-strategy generation on a language it never saw pretrained, and
the competence generalizes across time and asset — because what was learned is
the LANGUAGE, which is regime- and asset-independent.

DOES NOT establish EDGE. The verifier certifies well-formed and active, never
profitable. The 1.6-point train-vs-held-out gap correctly reads as "no
out-of-sample teeth" — here that is the honest signature of grammar competence
(the language is genuinely as easy on ETH June as on BTC January), NOT of
generalizing skill at finding good strategies. Selecting the profitable
strategy from this now-competent generator stays §5's job (`pick_verified`),
not the generator's. Fuel over survivors stayed at ~1% of the 25,000 cap
throughout — the termination bound was free on this task, as in the corpus run.

Per-style held-out gate-clear on the cross-asset window is near-uniform
(88-100%), so no collapse to one easy family.
