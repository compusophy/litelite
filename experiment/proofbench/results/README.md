# N=2 result — the same verifier-only method, a structurally different language

The §5 fine-tune, run a SECOND time on `prooflite` — a total, fuel-bounded
*compute* language with no market data, no trading, no candles. Same
language-parametric trainer, same rejection-sampling self-play, same
anti-collapse admission; only the reward binary changed (`s5` → `p6`). If the
stratlite result were a quirk of one grammar, it would not transfer. It does.
That is what turns an anecdote into a method.

## What reproduces, and what does not

As with N=1, the MODEL is not reproducible (stochastic sampling; a fine-tune
is not bit-identical across hardware). The SCORING is: the pools
(`base.jsonl`, `c6.jsonl`, `c7.jsonl`, `c8.jsonl` — 256 samples each, 32 per
style) are committed, and

    cd experiment/proofbench && ./target/release/p6 eval    results/c6.jsonl
    cd experiment/proofbench && ./target/release/p6 novelty results/c6.jsonl corpus/seed.jsonl

reproduce every number below (full output in `benchmark.txt`).

## The numbers

prooflite reads no data, so there is no train/held-out DATA split to measure a
generalization gap across (stratlite's three windows). The honest analog is
NOVELTY: are the model's rich programs LEARNED, or MEMORIZED from the 174
human-authored cold-start examples — the only external data it ever saw? The
validity ladder scores each program (parse → run-clean → gate → **ok/RICH**:
runs clean AND prints ≥3 distinct lines burning ≥30 fuel).

| model | parse | RICH (ok) | distinct rich / 256 | novel rich (∉ corpus) |
|---|---|---|---|---|
| base Qwen3-0.6B | 23.4% | **3.5%** | 9 | 9 / 9 (100%) |
| **C6 (selected)** | 99.2% | **94.5%** | **216** | 242 / 242 (**100%**) |
| C7 | 100.0% | 96.1% | 199 | 245 / 246 (99.6%) |
| C8 | 98.8% | 96.1% | 205 | 245 / 246 (99.6%) |

Two facts carry the result. First, the LIFT: rich-generation goes from 3.5% to
~95% — a small model taken from near-zero to competent by its verifier alone.
Second, NOVELTY: ~100% of the rich programs are source-canonically absent from
the cold-start corpus. The model did not memorize the 174 examples; it learned
to WRITE prooflite. (The novelty key is FNV over comment-stripped,
whitespace-collapsed source — it sees through format/comment clones.)

## Why the baseline is a true floor, not a weak one

prooflite exists in no pretraining corpus. Base Qwen3-0.6B parses 23.4% of its
attempts but writes a RICH program only 3.5% of the time (9 of 256) — it
recognizes the C-like surface (so it parses more than stratlite's 0%) but
cannot compose real, varied computation. Surface familiarity transfers from
pretraining; competence does not. That 3.5% → 94.5% gap is the fine-tune,
with nothing to confound it — and, like stratlite, it is the tempo-x402 result
(compiler-verified self-play, 0.5B model, 1.5% → 16.4% on a Rust benchmark)
generalized off `rustc` onto a purpose-built language.

## The training curve, and where it peaks (`../../train/` histograms)

Cold start on the 174 corpus survivors, then verifier-only self-play. Rich-rate
per round (ok of 1024 sampled):

    cold start -> R0 43.8% -> R1 63.5% -> R2 72.4% -> R3 83.1% -> R4 85.7%
    -> R5 90.7% -> R6 91.2% -> R7 95.0% -> R8 96.2%

Raw validity climbs monotonically. But DIVERSITY does not: admitted distinct
keys peak at round 6 (823), then fall (750, 686) as the policy concentrates on
a narrower band of high-reward programs — mild mode-narrowing that the
anti-collapse guards bound but do not abolish. The held-out benchmark confirms
the peak on FRESH samples: C6 emits **216** distinct rich programs, more than
C7 (199) or C8 (205), even though C7/C8 edge it on raw rich-rate. So C6 is the
selected checkpoint — validity is saturated across C6–C8, and C6 is where the
generator is most diverse.

## What this establishes — and what it does not

ESTABLISHES: the verifier-only fine-tune is a METHOD, not a one-grammar
artifact. Run unchanged on a language with no data, no market, no trading —
only checked arithmetic and bounded loops — it takes the same small model from
3.5% to ~95% rich generation, and the programs are ~100% novel. Two
independent purpose-sized languages, one recipe.

DOES NOT establish that every generated program is INTERESTING — only that it
is well-formed, runs, and prints real varied output (the RICH rung's bar).
Selecting genuinely useful programs from this now-competent generator is a
downstream concern, exactly as `pick_verified` is for stratlite. And the
diversity peak is a real caveat: past round 6 the generator trades breadth for
reward, so "train longer" is not free — the method has an optimal stop, and it
is observable in the distinct-key curve, not the rich-rate.

The rising-limb pool (c5) was lost to a cleanup slip and is regenerable in
~90s; the floor → peak → decline result stands on base/c6/c7/c8.
