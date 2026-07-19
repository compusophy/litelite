# N=2 result — the same verifier-only recipe, a structurally different language

The §5 fine-tune, run a SECOND time on `prooflite` — a total, fuel-bounded
*compute* language with no market data, no trading, no candles. Same
language-parametric trainer, same rejection-sampling self-play, same
anti-collapse admission; only the reward binary changed (`s5` → `p6`). If the
stratlite result were a quirk of one grammar, it would not transfer. It does.
That is what turns an anecdote into a method.

## What reproduces, and what does not

As with N=1, the MODEL is not reproducible (stochastic sampling; a fine-tune
is not bit-identical across hardware). The SCORING is: the pools
(`base.jsonl`, `c5.jsonl`, `c6.jsonl`, `c7.jsonl`, `c8.jsonl` — 256 samples each, 32 per
style) are committed, and

    cd experiment/proofbench && cargo build --release   # builds p6 (target/ is git-ignored)
    ./target/release/p6 eval    results/c6.jsonl
    ./target/release/p6 novelty results/c6.jsonl corpus/seed.jsonl

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
| Cinit (cold-start SFT only) | 83.2% | 48.8% | 124 | 124 / 125 (99.2%) |
| C5 | 96.9% | 90.6% | 213 | 232 / 232 (100%) |
| **C6 (selected)** | 99.2% | **94.5%** | **216** | 242 / 242 (**100%**) |
| C7 | 100.0% | 96.1% | 199 | 245 / 246 (99.6%) |
| C8 | 98.8% | 96.1% | 205 | 245 / 246 (99.6%) |

Two facts carry the result. First, the LIFT: rich-generation goes from 3.5% to
~95% — a small model taken from near-zero to competent by its verifier alone.
Second, NOVELTY: ~100% of the rich programs are source-canonically absent from
the cold-start corpus — so the competence is not recall of the 174 human
examples, the only external text the model saw. (The novelty key is FNV over
comment-stripped, whitespace-collapsed source — it sees through format/comment
clones. Note the scope: novelty is measured against the human seed only, not the
model's own self-play programs, so it rules out memorizing the human corpus, not
reproduction from the larger self-generated training set.)

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
keys peak at round 6 (823), then fall monotonically (750, 686) as the policy
concentrates on a narrower band of high-reward programs — mild mode-narrowing
that the anti-collapse guards bound but do not abolish. C6 also holds the most
distinct rich programs on the fresh benchmark (**216**, vs C7's 199 and C8's
205), so it is the selected checkpoint on both measures. Read the benchmark with
care, though: it is a single 256-sample draw per checkpoint, and its C7-vs-C8
order (205 > 199) inverts the training curve's (750 > 686) — past the peak the
two disagree within sampling noise, so the load-bearing evidence for the
narrowing is the training-curve decline, not the benchmark.

## What this establishes — and what it does not

ESTABLISHES: the verifier-only fine-tune is not a ONE-grammar artifact. Run
unchanged on a language with no data, no market, no trading — only checked
arithmetic and bounded loops — it takes the same small model from 3.5% to ~95%
rich generation, and the programs are ~100% novel against the human corpus. It
does NOT establish generality past the confounds both arms share (§4.5: same
kit, same author, same base `Qwen3-0.6B`, trainer, and reward shape) — cross-
model generality in particular is untested. N = 2 rules out one-grammar
specificity, not a recipe that generalizes to any model or language.

DOES NOT establish that every generated program is INTERESTING — only that it
is well-formed, runs, and prints real varied output (the RICH rung's bar).
Selecting genuinely useful programs from this now-competent generator is a
downstream concern, exactly as `pick_verified` is for stratlite. And the
diversity peak is a real caveat: past round 6 the generator trades breadth for
reward, so "train longer" is not free — the method has an optimal stop, and it
is observable in the distinct-key curve, not the rich-rate.

The full curve is committed — base → C5 → C6 (peak) → C7 → C8. C6 holds the
diversity peak (216 distinct rich), with C5 (213) just below it on the rising
limb and C7/C8 past it (199, 205), an inverted-U consistent with the training
histogram's distinct_nkeys peak at round 6.

## Transfer: does generation competence become problem-SOLVING competence?

The sharper question (`../problems/`): given a held-out SPEC, can the model
produce a program whose *output is correct* — not merely valid? 30 tiered
problems, 8 samples each (`solve_*.jsonl`, committed), scored by exact output
match against a verified reference (`p6 solve problems/heldout.jsonl
results/solve_<m>.jsonl`). Three arms: base, **Cinit** (cold-start on the 174
human programs = plain SFT, no self-play), **C6** (Cinit + 6 rounds of
verifier-only self-play — the selected generation checkpoint).

| model | RICH generation | solve pass@8 | easy | medium | hard | `pad` (right answer + extra output) |
|---|---|---|---|---|---|---|
| base | 3.5% | 20.0% (6/30) | 3/9 | 2/9 | 1/12 | 1 |
| **Cinit** | 48.8% | **80.0%** (24/30) | 9/9 | 8/9 | 7/12 | 1 |
| C6 | 94.5% | 43.3% (13/30) | 6/9 | 5/9 | 2/12 | **8** |

Two results, one expected and one not.

**Transfer is real.** Fine-tuning on verifier-selected programs turns a 20%
solver into an 80% solver — including 7 of the 12 `hard` discriminators
(primality, popcount, digit-sum) that need real algorithms. The model was never
trained to solve problems; it was trained to write valid programs. Solving
competence came along.

**Self-play optimizes the verifier, and the verifier is not the task.** C6 —
the checkpoint that WINS the generation benchmark (94.5% rich vs Cinit's 48.8%) —
LOSES half of Cinit's solving ability (43.3% vs 80%). The `pad` column says
why: 8 of C6's 17 misses computed the exactly correct answer, then appended
spurious prints. That is the RICH rung (≥3 distinct output lines, ≥30 fuel)
speaking — self-play distilled the reward's shape into an unconditional output
habit that overrides the spec. The localization is exact: on the 12 problems
whose correct output is itself rich (≥3 distinct lines), C6 matches Cinit
(9/12 vs 10/12); on the 18 problems whose correct output is SHORT — where the
training reward actively disprefers the correct shape — Cinit solves 14/18
while C6 solves 4/18 and pads 8 of the rest. Where reward and task agree,
self-play costs nothing; where they conflict, the policy obeys its internalized
reward, not the prompt.

So the two capabilities MOVE OPPOSITE under self-play: generation validity
3.5→48.8→94.5, spec-conditioned solving 20→80→43. The plain-SFT checkpoint is
the better *solver*; the self-play checkpoint is the better *generator*. This
is Goodhart at the policy level, measured: a verifier certifies validity, not
intent, and optimizing against it long enough imprints its preferences as
priors that persist even when the prompt asks for something else. The honest
recipe that falls out: cold-start SFT buys the language; self-play buys the
verifier's rungs — run it only as long as the rungs and the downstream task
agree, and read the `pad` column as the early-warning signal.

(Caveats: pass@8 on 30 problems is a coarse instrument — single-draw, one
model family, one language; `pad` catches prefix-shaped padding only, so the
"obeys reward over prompt" mass is a lower bound. The base row's 20% shows the
problems are not unreachable from pretraining alone — surface familiarity
solves a few easy ones.)

## The fix arm: spec-conditioned self-play (S1–S5)

If the deficit above is really the reward's shape imprinting, then changing
WHAT the verifier rewards — nothing else — should move it. This arm does
exactly that: same trainer, same admission guards, same Cinit start; every
sampling prompt is now a training task ("a program that {spec}",
`problems/train.jsonl` — 48 specs with verified references, ids disjoint from
the 30 held-out problems and NO family overlap with the held-out hard tier:
no primes, popcount, digit-sum/reverse, Collatz-length, fib, powers, or
square/cube/odd sums), and the reward's top rung is CORRECTNESS — exact
output match against that spec's reference (`p6 solvereward`) — instead of
RICH shape. Six rounds; correct-of-768 on train climbed 27.0% → 64.3% →
81.3% → 88.7% → 93.0% → 93.1% (`train_curve_solve.log`; admitted distinct
keys peak at round 3 — the inverted-U again).

| model | reward trained on | RICH generation | solve pass@8 | hard tier | `pad` |
|---|---|---|---|---|---|
| Cinit | (none — plain SFT) | 48.8% | 80.0% | 7/12 | 1 |
| C6 | RICH shape | **94.5%** | 43.3% | 2/12 | 8 |
| **S5** | spec correctness | 24.2% | **93.3%** | **10/12** | 1 |

Three facts.

**The fix works, and overshoots.** S5 solves 28/30 (easy 9/9, medium 9/9,
hard 10/12) — not merely recovering Cinit's 80% but beating it by 13 points,
with one round of correctness self-play (S1, 86.7%) already enough to pass
plain SFT. The padding habit is gone (1 `pad`, vs C6's 8).

**The hard-tier gain is transfer, not leakage.** The training specs share no
family with the held-out hard problems, yet S5 takes the hard tier from
Cinit's 7/12 to 10/12 — alternating sums, primality, fib(12) all newly
solved. Composing loop-accumulator-conditional skills on 48 disjoint tasks
transferred to algorithm families the model was never trained to solve. The
train-vs-held-out gap is 97.9% vs 93.3% (4.6 points) — real but small.

**The symmetry is the finding.** Each self-play arm maximized exactly its own
reward and DEGRADED the other axis relative to Cinit: C6 is 94.5% RICH /
43.3% solve, S5 is 24.2% RICH / 93.3% solve (its unconditional generations
are short, correct-shaped, terse — parse 91.0%, but only 24.2% clear the ≥3
distinct lines + ≥30 fuel bar). Self-play does not "make the model better";
it makes the model MORE LIKE ITS REWARD, everywhere, including contexts the
reward never saw. Which capability you get is decided entirely by which
predicate you hand the verifier — and both arms' scoring reproduces from the
committed pools (`solve_s{1,3,5}.jsonl`, `solve_s5_train.jsonl`,
`s5gen.jsonl`; `p6 solve` / `p6 eval`).
