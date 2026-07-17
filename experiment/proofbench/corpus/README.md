# experiment/proofbench/corpus — a verified prooflite corpus (N=2 cold start)

`seed.jsonl` is 174 prooflite programs (`{id, style_idx, source}`), generated
with NO API key: eight Opus agents, one per computation family (arithmetic,
if/else-if chains, repeat accumulators, boolean logic, sequences, stateful
loops, guarded division, nested loops), each given the p6 prompt card. The real
prooflite engine (`p6`) then scored them; only ok-rung survivors are kept.

The prooflite analog of `experiment/corpus/` — the cold-start set for the N=2
arm of the fine-tune experiment. Reproduce the scoring:

    cd experiment/proofbench && ./target/release/p6 eval corpus/seed.jsonl

## Verified against the real engine

The 175 raw agent programs scored (`p6 eval`): **100% parse, 100% run-clean,
99.4% RICH** (174 ok, 1 gate, 0 run-fault, 0 compile-fail). Agents plus the
card write valid, non-trivial prooflite almost perfectly. The one gate program
(printed too few distinct values) is dropped; the 174 ok programs, all with
distinct source-canonical keys, are the committed cold-start set.

Contrast with the base model this bootstraps: base Qwen3-0.6B writes prooflite
that PARSES 25.8% of the time but is RICH only 2.3% of the time (see
`../../results` once the run lands). Surface familiarity transfers from
pretraining; competence does not — that gap is what the fine-tune closes.
